//! One run of the agent: the loop of "ask the model, run the tools it calls, ask again".
//!
//! The run lives in this process; the page only sees events (text as it streams, each tool call, usage)
//! and answers tool calls, because the tools read what the page already holds of the workspace. The
//! model's calls are checked against the closed table here before they reach the page, a tool's answer
//! is cut and cleaned before it reaches the model, and nothing the model says can add a tool, decide an
//! approval, run a command or name an address.

use super::client::{self, Limits};
use super::config::{self, AgentConfig};
use super::tools;
use super::types::{AgentError, ApiKey, Endpoint, Msg, Stop, ToolCall, ToolDecl, Usage};
use super::wire;
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use tokio::sync::watch;

pub const MAX_STEPS: u32 = 8;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Text {
        delta: String,
    },
    Call {
        id: String,
        name: String,
        args: Value,
    },
    Refused {
        id: String,
        name: String,
        reason: String,
    },
    Usage {
        input: u64,
        output: u64,
        cost_micros: Option<u64>,
        thread_tokens: u64,
        day_tokens: u64,
    },
    Done {
        stop: String,
        steps: u32,
    },
    Error {
        kind: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct ToolReply {
    pub output: String,
    pub refused: Option<String>,
}

/// What a run needs from its surroundings: where events go, how a tool call is answered, and the ledger.
pub trait Io: Send {
    fn emit(&mut self, event: Event);
    fn tool(&mut self, call: &ToolCall) -> impl Future<Output = ToolReply> + Send;
    /// Tokens used in this thread, tokens and estimated cost (millionths of a dollar) in the last 24 hours.
    fn totals(&mut self, thread: &str) -> impl Future<Output = (u64, u64, u64)> + Send;
    fn record(
        &mut self,
        thread: &str,
        provider: &str,
        model: &str,
        usage: Usage,
        cost: Option<u64>,
    ) -> impl Future<Output = ()> + Send;
}

pub struct RunInput {
    pub thread: String,
    pub provider_id: String,
    pub endpoint: Endpoint,
    pub system: String,
    pub history: Vec<Msg>,
    pub tools: Vec<ToolDecl>,
}

fn kind_of(e: &AgentError) -> &'static str {
    match e {
        AgentError::Provider { .. } => "provider",
        AgentError::Rejected(_) => "rejected",
        AgentError::RateLimited { .. } => "rate_limited",
        AgentError::Unreachable => "unreachable",
        AgentError::TimedOut => "timed_out",
        AgentError::Cancelled => "cancelled",
        AgentError::TooLarge => "too_large",
        AgentError::BadAnswer(_) => "bad_answer",
        AgentError::NoKey => "no_key",
        AgentError::NoConsent => "no_consent",
        AgentError::Offline => "offline",
        AgentError::CapReached(_) => "cap",
        AgentError::BadSetup(_) => "setup",
    }
}

/// Runs to the end of the model's answer. Everything the person should see goes out as events; the
/// final error (if any) is also returned.
pub async fn run<I: Io>(
    io: &mut I,
    cfg: &AgentConfig,
    key: Option<ApiKey>,
    input: RunInput,
    limits: Limits,
    cancel: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let outcome = run_inner(io, cfg, key, input, limits, cancel).await;
    if let Err(e) = &outcome {
        io.emit(Event::Error {
            kind: kind_of(e).to_string(),
            message: e.to_string(),
        });
    }
    outcome
}

async fn run_inner<I: Io>(
    io: &mut I,
    cfg: &AgentConfig,
    key: Option<ApiKey>,
    input: RunInput,
    limits: Limits,
    cancel: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let ep = &input.endpoint;
    config::allow(cfg, &input.provider_id, &ep.host())?;
    tools::validate(&input.tools).map_err(AgentError::BadSetup)?;
    if ep.kind.needs_key() && key.is_none() {
        return Err(AgentError::NoKey);
    }
    let mut msgs = input.history.clone();
    for step in 1..=MAX_STEPS {
        let (thread_tokens, day_tokens, day_cost) = io.totals(&input.thread).await;
        config::within_caps(cfg, thread_tokens, day_tokens, day_cost)?;
        let built = wire::build(ep, &input.system, &msgs, &input.tools);
        // Text goes to the page as it streams.
        let turn = {
            let mut on_text = |t: &str| {
                io.emit(Event::Text {
                    delta: t.to_string(),
                })
            };
            client::send_turn(
                ep,
                key.as_ref(),
                &built,
                cancel.clone(),
                limits,
                &mut on_text,
            )
            .await
        };
        let turn = turn?;
        let cost = config::cost(cfg, &ep.model, turn.usage.input, turn.usage.output);
        io.record(
            &input.thread,
            &input.provider_id,
            &ep.model,
            turn.usage,
            cost,
        )
        .await;
        let (t2, d2, _) = io.totals(&input.thread).await;
        io.emit(Event::Usage {
            input: turn.usage.input,
            output: turn.usage.output,
            cost_micros: cost,
            thread_tokens: t2,
            day_tokens: d2,
        });
        msgs.push(Msg::Assistant {
            text: turn.text.clone(),
            calls: turn.calls.clone(),
            raw: turn.raw.clone(),
        });
        if turn.stop != Stop::ToolCalls || turn.calls.is_empty() {
            let stop = match turn.stop {
                Stop::End | Stop::ToolCalls => "end",
                Stop::Length => "length",
                Stop::Other => "other",
            };
            io.emit(Event::Done {
                stop: stop.to_string(),
                steps: step,
            });
            return Ok(());
        }
        for call in &turn.calls {
            let declared = input.tools.iter().any(|t| t.name == call.name);
            let output = if !tools::in_table(&call.name) || !declared {
                let reason = if tools::in_table(&call.name) {
                    "it was not offered in this thread".to_string()
                } else {
                    "there is no such tool".to_string()
                };
                io.emit(Event::Refused {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    reason: reason.clone(),
                });
                format!("refused: {reason}. The tools are only the ones listed.")
            } else {
                io.emit(Event::Call {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    args: call.args.clone(),
                });
                let reply = io.tool(call).await;
                match reply.refused {
                    Some(why) => format!("refused: {why}"),
                    None => reply.output,
                }
            };
            msgs.push(Msg::ToolResult {
                call_id: call.id.clone(),
                name: call.name.clone(),
                output: tools::clip(&output),
            });
        }
    }
    io.emit(Event::Done {
        stop: "steps".into(),
        steps: MAX_STEPS,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::config::Consent;
    use super::super::testserver::{self, Reply};
    use super::super::types::Kind;
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    const CANARY: &str = "DYNODOC-CANARY-sk-0123456789abcdefghij";

    struct Mock {
        events: Vec<Event>,
        replies: Vec<ToolReply>,
        asked: Vec<ToolCall>,
        used: (u64, u64, u64),
        ledger: Vec<(String, String, Usage, Option<u64>)>,
    }

    impl Mock {
        fn new(replies: Vec<ToolReply>) -> Self {
            Self {
                events: vec![],
                replies,
                asked: vec![],
                used: (0, 0, 0),
                ledger: vec![],
            }
        }
    }

    impl Io for Mock {
        fn emit(&mut self, event: Event) {
            self.events.push(event);
        }
        async fn tool(&mut self, call: &ToolCall) -> ToolReply {
            self.asked.push(call.clone());
            if self.replies.is_empty() {
                ToolReply {
                    output: "none".into(),
                    refused: None,
                }
            } else {
                self.replies.remove(0)
            }
        }
        async fn totals(&mut self, _: &str) -> (u64, u64, u64) {
            self.used
        }
        async fn record(
            &mut self,
            _: &str,
            provider: &str,
            model: &str,
            usage: Usage,
            cost: Option<u64>,
        ) {
            self.used.0 += usage.input + usage.output;
            self.used.1 += usage.input + usage.output;
            self.ledger
                .push((provider.into(), model.into(), usage, cost));
        }
    }

    fn cfg_for(host: &str) -> AgentConfig {
        let mut c = AgentConfig::default();
        c.consent.insert(
            "lp1".into(),
            Consent {
                host: host.into(),
                at: 1,
            },
        );
        c
    }

    fn decl(name: &str) -> ToolDecl {
        ToolDecl {
            name: name.into(),
            description: "d".into(),
            parameters: json!({ "type": "object" }),
        }
    }

    fn input(kind: Kind, base: &str, tools: Vec<ToolDecl>) -> RunInput {
        RunInput {
            thread: "th_1".into(),
            provider_id: "lp1".into(),
            endpoint: Endpoint::parse(kind, base, "test-model").unwrap(),
            system: "Be brief.".into(),
            history: vec![Msg::User("Summarise #1".into())],
            tools,
        }
    }

    fn quick() -> Limits {
        Limits {
            connect: Duration::from_secs(2),
            idle: Duration::from_secs(5),
            total: Duration::from_secs(20),
            retries: 2,
            base_backoff: Duration::from_millis(5),
        }
    }

    fn anthropic_tool_turn() -> String {
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":100,\"output_tokens\":1}}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"requests__summarise\",\"input\":{}}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"number\\\": 1}\"}}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":20}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string()
    }

    fn anthropic_final_turn() -> String {
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":160,\"output_tokens\":1}}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Request 1 changes one sentence.\"}}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":9}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string()
    }

    #[tokio::test]
    async fn a_run_calls_a_tool_and_answers_and_the_key_goes_only_in_its_header() {
        let server = testserver::start(vec![
            Reply::Sse(vec![anthropic_tool_turn()], Duration::from_millis(1)),
            Reply::Sse(vec![anthropic_final_turn()], Duration::from_millis(1)),
        ])
        .await;
        let base = server.base();
        let mut io = Mock::new(vec![ToolReply {
            output: "1 change; 0/0 approvals".into(),
            refused: None,
        }]);
        let (_tx, rx) = watch::channel(false);
        let key = ApiKey::new(CANARY);
        run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", server.addr.port())),
            key,
            input(Kind::Anthropic, &base, vec![decl("requests.summarise")]),
            quick(),
            rx,
        )
        .await
        .unwrap();
        // The tool the model called was asked for by its own name, with its arguments.
        assert_eq!(io.asked.len(), 1);
        assert_eq!(io.asked[0].name, "requests.summarise");
        assert_eq!(io.asked[0].args, json!({ "number": 1 }));
        let text: String = io
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Text { delta } = e {
                    Some(delta.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "Request 1 changes one sentence.");
        assert!(matches!(io.events.last(), Some(Event::Done { stop, steps: 2 }) if stop == "end"));
        assert_eq!(io.ledger.len(), 2);
        // The second request carried the tool's answer back.
        let seen = server.requests();
        assert_eq!(seen.len(), 2);
        assert!(seen[1].body.contains("1 change; 0/0 approvals"));
        // The key: in the key header and nowhere else (not in a body, a path, an event, the ledger).
        assert_eq!(seen[0].header("x-api-key"), Some(CANARY));
        for r in &seen {
            assert!(!r.body.contains("CANARY") && !r.path.contains("CANARY"));
            assert!(
                r.headers
                    .iter()
                    .filter(|(k, v)| v.contains("CANARY") && !k.eq_ignore_ascii_case("x-api-key"))
                    .count()
                    == 0
            );
        }
        let shown = format!("{:?}{:?}", io.events, io.ledger);
        assert!(!shown.contains("CANARY"));
    }

    #[tokio::test]
    async fn a_tool_outside_the_table_or_not_offered_is_refused_and_never_reaches_the_page() {
        let turn = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"requests__merge\",\"arguments\":\"{\\\"number\\\":1}\"}},{\"index\":1,\"id\":\"c2\",\"function\":{\"name\":\"diff__get\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n".to_string();
        let done = "data: {\"choices\":[{\"delta\":{\"content\":\"I cannot merge.\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":4}}\n\ndata: [DONE]\n\n".to_string();
        let server = testserver::start(vec![
            Reply::Sse(vec![turn], Duration::from_millis(1)),
            Reply::Sse(vec![done], Duration::from_millis(1)),
        ])
        .await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", server.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Compat, &server.base(), vec![decl("requests.list")]),
            quick(),
            rx,
        )
        .await
        .unwrap();
        assert!(
            io.asked.is_empty(),
            "no call reached the page: {:?}",
            io.asked
        );
        let refused: Vec<&Event> = io
            .events
            .iter()
            .filter(|e| matches!(e, Event::Refused { .. }))
            .collect();
        assert_eq!(refused.len(), 2);
        let second = &server.requests()[1];
        assert!(
            second.body.contains("refused: there is no such tool")
                && second.body.contains("not offered")
        );
    }

    #[tokio::test]
    async fn a_declaration_outside_the_table_stops_the_run_before_any_request() {
        let server = testserver::start(vec![Reply::Plain(200, vec![], String::new())]).await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        let e = run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", server.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Compat, &server.base(), vec![decl("requests.merge")]),
            quick(),
            rx,
        )
        .await
        .unwrap_err();
        assert!(matches!(e, AgentError::BadSetup(_)));
        assert!(server.requests().is_empty());
    }

    #[tokio::test]
    async fn no_consent_offline_no_key_and_caps_stop_a_run_before_the_network() {
        let server = testserver::start(vec![Reply::Plain(200, vec![], String::new())]).await;
        let host = format!("127.0.0.1:{}", server.addr.port());
        let go = |cfg: AgentConfig, key: Option<ApiKey>, used: (u64, u64, u64)| {
            let base = server.base();
            async move {
                let mut io = Mock::new(vec![]);
                io.used = used;
                let (_tx, rx) = watch::channel(false);
                let r = run(
                    &mut io,
                    &cfg,
                    key,
                    input(Kind::Compat, &base, vec![]),
                    quick(),
                    rx,
                )
                .await;
                (r, io.events)
            }
        };
        let (r, ev) = go(AgentConfig::default(), ApiKey::new(CANARY), (0, 0, 0)).await;
        assert_eq!(r, Err(AgentError::NoConsent));
        assert!(matches!(ev.last(), Some(Event::Error { kind, .. }) if kind == "no_consent"));
        let mut off = cfg_for(&host);
        off.offline = true;
        assert_eq!(
            go(off, ApiKey::new(CANARY), (0, 0, 0)).await.0,
            Err(AgentError::Offline)
        );
        assert_eq!(
            go(cfg_for(&host), None, (0, 0, 0)).await.0,
            Err(AgentError::NoKey)
        );
        let mut zero = cfg_for(&host);
        zero.thread_token_cap = 0;
        assert!(matches!(
            go(zero, ApiKey::new(CANARY), (0, 0, 0)).await.0,
            Err(AgentError::CapReached(_))
        ));
        assert!(matches!(
            go(cfg_for(&host), ApiKey::new(CANARY), (500_000, 0, 0))
                .await
                .0,
            Err(AgentError::CapReached(_))
        ));
        assert!(
            server.requests().is_empty(),
            "none of these reached the provider"
        );
    }

    #[tokio::test]
    async fn a_rate_limit_is_retried_and_a_key_error_is_not_and_never_shows_the_key() {
        let ok = "data: {\"choices\":[{\"delta\":{\"content\":\"fine\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n".to_string();
        let server = testserver::start(vec![
            Reply::Plain(
                429,
                vec![("retry-after", "0")],
                "{\"error\":{\"message\":\"slow down\"}}".into(),
            ),
            Reply::Plain(503, vec![], "{}".into()),
            Reply::Sse(vec![ok], Duration::from_millis(1)),
        ])
        .await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", server.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Openai, &server.base(), vec![]),
            quick(),
            rx,
        )
        .await
        .unwrap();
        assert_eq!(server.requests().len(), 3, "two retries, then the answer");
        assert_eq!(
            server.requests()[0].header("authorization"),
            Some(&*format!("Bearer {CANARY}"))
        );

        let denied = testserver::start(vec![Reply::Plain(
            401,
            vec![],
            format!("{{\"error\":{{\"message\":\"Incorrect API key provided: {CANARY}\"}}}}"),
        )])
        .await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        let e = run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", denied.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Openai, &denied.base(), vec![]),
            quick(),
            rx,
        )
        .await
        .unwrap_err();
        assert_eq!(denied.requests().len(), 1, "a refused key is not retried");
        let shown = format!("{e} {:?}", io.events);
        assert!(
            shown.contains("not accepted") && !shown.contains("CANARY"),
            "{shown}"
        );
    }

    #[tokio::test]
    async fn stopping_a_run_stops_the_stream() {
        let chunks: Vec<String> = (0..50)
            .map(|i| format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"w{i} \"}}}}]}}\n\n"))
            .collect();
        let server = testserver::start(vec![Reply::Sse(chunks, Duration::from_millis(40))]).await;
        let mut io = Mock::new(vec![]);
        let (tx, rx) = watch::channel(false);
        let host = format!("127.0.0.1:{}", server.addr.port());
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let _ = tx.send(true);
        });
        let started = std::time::Instant::now();
        let e = run(
            &mut io,
            &cfg_for(&host),
            ApiKey::new(CANARY),
            input(Kind::Compat, &server.base(), vec![]),
            quick(),
            rx,
        )
        .await
        .unwrap_err();
        stopper.await.unwrap();
        assert_eq!(e, AgentError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn a_silent_provider_times_out_and_gemini_and_ollama_work_without_a_header_the_others_need(
    ) {
        let silent = testserver::start(vec![Reply::Silent]).await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        let limits = Limits {
            idle: Duration::from_millis(300),
            total: Duration::from_millis(600),
            retries: 0,
            ..quick()
        };
        let e = run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", silent.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Compat, &silent.base(), vec![]),
            limits,
            rx,
        )
        .await
        .unwrap_err();
        assert_eq!(e, AgentError::TimedOut);

        let g = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":1}}\n\n".to_string();
        let gem = testserver::start(vec![Reply::Sse(vec![g], Duration::from_millis(1))]).await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        let base = format!("{}/v1beta", gem.base());
        run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", gem.addr.port())),
            ApiKey::new(CANARY),
            input(Kind::Google, &base, vec![]),
            quick(),
            rx,
        )
        .await
        .unwrap();
        let r = &gem.requests()[0];
        assert_eq!(
            r.path,
            "/v1beta/models/test-model:streamGenerateContent?alt=sse"
        );
        assert_eq!(r.header("x-goog-api-key"), Some(CANARY));
        assert!(!r.path.contains("key="));

        let o = "data: {\"choices\":[{\"delta\":{\"content\":\"local\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_string();
        let oll = testserver::start(vec![Reply::Sse(vec![o], Duration::from_millis(1))]).await;
        let mut io = Mock::new(vec![]);
        let (_tx, rx) = watch::channel(false);
        run(
            &mut io,
            &cfg_for(&format!("127.0.0.1:{}", oll.addr.port())),
            None,
            input(Kind::Ollama, &oll.base(), vec![]),
            quick(),
            rx,
        )
        .await
        .unwrap();
        let r = &oll.requests()[0];
        assert_eq!(r.path, "/v1/chat/completions");
        assert!(
            r.header("authorization").is_none(),
            "Ollama needs no key and is sent none"
        );
    }
    #[tokio::test]
    #[ignore = "real Claude subscription request; VERNE_TEST_SUBSCRIPTION=claude"]
    async fn native_subscription_provider_live() {
        assert_eq!(std::env::var("VERNE_TEST_SUBSCRIPTION").as_deref(),Ok("claude"));
        let base="https://claude.local-agent.invalid/v1";
        let mut request=input(Kind::Claude,base,vec![]);
        request.endpoint.model="default".into();
        request.system="Reply with the single word OK.".into();
        request.history=vec![Msg::User("ping".into())];
        let cfg=cfg_for("claude.local-agent.invalid");
        let mut io=Mock::new(vec![]);
        let (_tx,cancel)=watch::channel(false);
        let mut limits=quick();limits.total=Duration::from_secs(120);
        run(&mut io,&cfg,None,request,limits,cancel).await.unwrap();
        let text=io.events.iter().filter_map(|e|match e { Event::Text{delta}=>Some(delta.as_str()),_=>None }).collect::<String>();
        assert_eq!(text.trim(),"OK");
        assert_eq!(io.ledger.len(),1);
        assert!(io.events.iter().any(|e|matches!(e,Event::Done{..})));
    }

}
