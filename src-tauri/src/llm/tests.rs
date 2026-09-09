//! Phase 15 gate coverage — everything that does **not** need a real CUDA
//! `llama-server` binary + a real GGUF. Those items are deferred (plan 15.D).
//!
//! An in-process `tiny_http` server mimics `/health` + `/completion` (stream and
//! not) + the `--api-key` bearer check.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::client::LlamaClient;
use super::protocol::{parse_chunk, parse_sse_line, CompletionRequest, SseEvent};
use super::server::{bearer_token, pick_free_port, ServerArgs};
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::ipc::AppError;

const TOKEN: &str = "test-token-1234";

// ---------------------------------------------------------------- protocol

#[test]
fn sse_lines_parse() {
    assert_eq!(
        parse_sse_line("data: {\"content\":\"hi\"}\n"),
        Some(SseEvent::Data("{\"content\":\"hi\"}".to_owned()))
    );
    assert_eq!(parse_sse_line("data: [DONE]"), Some(SseEvent::Done));
    assert_eq!(parse_sse_line(": keep-alive"), None);
    assert_eq!(parse_sse_line(""), None);
    assert_eq!(parse_sse_line("event: message"), None);
}

#[test]
fn chunk_stop_reason_maps_every_variant() {
    let eos: super::protocol::CompletionChunk =
        parse_chunk(r#"{"content":"","stop":true,"stop_type":"eos"}"#).unwrap();
    assert_eq!(eos.stop_reason(), StopReason::EndOfText);
    let word = parse_chunk(r#"{"stop":true,"stop_type":"word"}"#).unwrap();
    assert_eq!(word.stop_reason(), StopReason::StopSequence);
    let limit = parse_chunk(r#"{"stop":true,"stop_type":"limit"}"#).unwrap();
    assert_eq!(limit.stop_reason(), StopReason::MaxTokens);
    // Legacy booleans.
    let legacy = parse_chunk(r#"{"stop":true,"stopped_limit":true}"#).unwrap();
    assert_eq!(legacy.stop_reason(), StopReason::MaxTokens);
}

#[test]
fn malformed_chunk_is_an_error() {
    assert!(matches!(
        parse_chunk("not json"),
        Err(AppError::BackendUnavailable(_))
    ));
}

// ---------------------------------------------------------------- server args

#[test]
fn free_port_is_bindable_and_in_range() {
    let port = pick_free_port().unwrap();
    assert!(port >= 1024);
    // Re-bind to prove it was actually released.
    std::net::TcpListener::bind(("127.0.0.1", port)).expect("port is free");
}

#[test]
fn bearer_token_is_64_hex_and_unique() {
    let a = bearer_token();
    let b = bearer_token();
    assert_eq!(a.len(), 64);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

#[test]
fn server_argv_carries_the_essentials() {
    let spec = ServerArgs {
        model_path: std::path::PathBuf::from("/models/m.gguf"),
        port: 55_123,
        token: "abc".to_owned(),
        n_gpu_layers: -1,
        ctx_size: 8192,
    };
    let joined = spec.to_argv().join(" ");
    assert!(joined.contains("--model /models/m.gguf"));
    assert!(joined.contains("--port 55123"));
    assert!(joined.contains("--api-key abc"));
    assert!(joined.contains("--n-gpu-layers -1"));
    assert!(joined.contains("--ctx-size 8192"));
    assert!(joined.contains("--no-webui"));
    // `--no-warmup` was intentionally dropped — llama.cpp's own warm-up pass
    // now runs, avoiding an extra latency spike on the first prompt.
    assert!(!joined.contains("--no-warmup"));
    assert_eq!(spec.base_url(), "http://127.0.0.1:55123");
}

// ---------------------------------------------------------------- stub server

/// What the stub should do for a `/completion` request.
#[derive(Clone, Copy)]
enum Mode {
    /// Non-streaming: one JSON body.
    Once,
    /// SSE: `words` deltas then a final stop object.
    Stream,
    /// Accept the connection then never respond (drives the deadline).
    Stall,
}

struct Stub {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Stub {
    fn start(mode: Mode) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let handle = thread::spawn(move || loop {
            if stop_thread.load(Ordering::SeqCst) {
                break;
            }
            let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(50)) else {
                continue;
            };
            let authed = req.headers().iter().any(|h| {
                h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {TOKEN}")
            });
            let url = req.url().to_owned();

            if !authed {
                let _ = req.respond(
                    tiny_http::Response::from_string(r#"{"error":{"message":"invalid api key"}}"#)
                        .with_status_code(401),
                );
                continue;
            }
            if url.starts_with("/health") {
                let _ = req.respond(tiny_http::Response::from_string("OK"));
                continue;
            }
            // /completion (the stub ignores the request body)
            match mode {
                Mode::Once => {
                    let _ = req.respond(tiny_http::Response::from_string(
                        r#"{"content":"hello world","stop":true,"stop_type":"eos","tokens_predicted":2}"#,
                    ));
                }
                Mode::Stream => {
                    let payload = concat!(
                        "data: {\"content\":\"hel\",\"stop\":false}\n\n",
                        "data: {\"content\":\"lo\",\"stop\":false}\n\n",
                        "data: {\"content\":\"\",\"stop\":true,\"stop_type\":\"eos\",\"tokens_predicted\":2}\n\n",
                    );
                    let resp = tiny_http::Response::from_string(payload).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"text/event-stream"[..],
                        )
                        .unwrap(),
                    );
                    let _ = req.respond(resp);
                }
                Mode::Stall => {
                    while !stop_thread.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(25));
                    }
                    drop(req);
                }
            }
        });
        Self {
            base_url,
            stop,
            handle: Some(handle),
        }
    }

    fn client(&self, deadline: Duration) -> LlamaClient {
        LlamaClient::new(self.base_url.clone(), TOKEN, deadline)
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn params() -> SamplingParams {
    SamplingParams {
        temperature: Some(0.7),
        top_p: None,
        top_k: None,
        max_tokens: Some(64),
        stop: vec![],
        seed: Some(1),
    }
}

// ---------------------------------------------------------------- client

#[tokio::test]
async fn health_ok_and_auth_rejected() {
    let stub = Stub::start(Mode::Once);
    let good = stub.client(Duration::from_secs(5));
    good.health().await.expect("health ok");

    let bad = LlamaClient::new(stub.base_url.clone(), "wrong", Duration::from_secs(5));
    let err = bad.health().await.unwrap_err();
    assert!(matches!(err, AppError::BackendUnavailable(_)));
}

#[tokio::test]
async fn non_streaming_completion_returns_text_and_stop_reason() {
    let stub = Stub::start(Mode::Once);
    let client = stub.client(Duration::from_secs(5));
    let req = CompletionRequest::from_params("hi".to_owned(), &params(), false);
    let out = client
        .complete(&req, &CancellationToken::new())
        .await
        .expect("completed");
    assert_eq!(out.text, "hello world");
    assert_eq!(out.tokens, 2);
    assert_eq!(out.stop_reason, StopReason::EndOfText);
}

#[tokio::test]
async fn streaming_completion_yields_ordered_deltas_then_done() {
    let stub = Stub::start(Mode::Stream);
    let client = stub.client(Duration::from_secs(5));
    let req = CompletionRequest::from_params("hi".to_owned(), &params(), true);
    let (tx, mut rx) = mpsc::channel(16);
    client.stream(&req, &tx, &CancellationToken::new()).await;

    let mut texts = Vec::new();
    let mut done = None;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            GenerationEvent::TokenDelta { index, text } => {
                assert_eq!(index as usize, texts.len());
                texts.push(text);
            }
            GenerationEvent::Done {
                stop_reason,
                tokens,
            } => done = Some((stop_reason, tokens)),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(texts, ["hel", "lo"]);
    assert_eq!(done, Some((StopReason::EndOfText, 2)));
}

#[tokio::test]
async fn a_mid_stream_cancel_emits_cancelled() {
    let stub = Stub::start(Mode::Stall);
    let client = stub.client(Duration::from_secs(30));
    let req = CompletionRequest::from_params("hi".to_owned(), &params(), true);
    let (tx, mut rx) = mpsc::channel(16);
    let cancel = CancellationToken::new();

    let c2 = cancel.clone();
    let streamer = tokio::spawn(async move { client.stream(&req, &tx, &c2).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    streamer.await.unwrap();

    assert!(matches!(rx.try_recv(), Ok(GenerationEvent::Cancelled)));
}

#[tokio::test]
async fn a_stalled_server_hits_the_deadline() {
    let stub = Stub::start(Mode::Stall);
    let client = stub.client(Duration::from_millis(200));
    let req = CompletionRequest::from_params("hi".to_owned(), &params(), false);
    let err = client
        .complete(&req, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Timeout(_)));
}

// ---------------------------------------------------------------- as_llm / instance

#[tokio::test]
async fn a_llama_server_instance_generates_and_downcasts() {
    use std::sync::Arc;

    use crate::lifecycle::backend::LoadedInstance;

    let stub = Stub::start(Mode::Once);
    let instance: Arc<dyn LoadedInstance> = Arc::new(super::LlamaServer::attached(
        stub.client(Duration::from_secs(5)),
        Some(4_096),
    ));
    instance.health().await.expect("healthy");
    assert_eq!(instance.measured_vram_mb(), Some(4_096));

    let llm = instance.as_llm().expect("is an LlmInstance");
    let out = llm
        .generate("hi".to_owned(), params(), CancellationToken::new())
        .await
        .expect("generated");
    assert_eq!(out.text, "hello world");

    instance.shutdown().await; // no process attached — must not panic
}

#[test]
fn job_object_can_be_created() {
    // Not llama-server, but proves the Job Object path builds + runs.
    let job = crate::job::JobObject::new().expect("job created");
    drop(job);
}
