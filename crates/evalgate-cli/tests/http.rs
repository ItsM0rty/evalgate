use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use evalgate_cli::{run_suite, RunOptions};
use evalgate_core::{parse_suite, GateSuite};
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};
use tempfile::TempDir;
use tokio::{net::TcpListener, sync::Mutex};

#[derive(Clone)]
struct ServerState {
    replies: Arc<Mutex<Vec<(StatusCode, Value)>>>,
    times: Arc<Mutex<Vec<Instant>>>,
}
async fn handler(State(state): State<ServerState>) -> (StatusCode, Json<Value>) {
    state.times.lock().await.push(Instant::now());
    let mut replies = state.replies.lock().await;
    let (status, body) = if replies.len() > 1 {
        replies.remove(0)
    } else {
        replies[0].clone()
    };
    (status, Json(body))
}
async fn server(replies: Vec<(StatusCode, Value)>) -> (String, ServerState) {
    let state = ServerState {
        replies: Arc::new(Mutex::new(replies)),
        times: Arc::new(Mutex::new(Vec::new())),
    };
    let app = Router::new()
        .route("/", post(handler))
        .with_state(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/"), state)
}
fn options(temp: &TempDir, no_cache: bool) -> RunOptions {
    RunOptions {
        root: temp.path().into(),
        no_cache,
        ..RunOptions::default()
    }
}
fn target_suite(kind: &str, url: &str, count: usize, extra: &str) -> GateSuite {
    let cases = (0..count).map(|i| format!("  - name: case{i}\n    prompt: hello {i}\n    checks: [{{kind: includes, value: ok}}]\n")).collect::<String>();
    parse_suite(&format!("suite: {{name: http}}\ntargets:\n  - name: api\n    type: {kind}\n    endpoint: {url}\n    {extra}\ncases:\n{cases}")).unwrap()
}

#[tokio::test]
async fn openai_target() {
    let (url, _) = server(vec![(
        StatusCode::OK,
        json!({"choices":[{"message":{"content":"ok openai"}}]}),
    )])
    .await;
    let suite = target_suite("openai", &url, 1, "concurrency: 1");
    let temp = TempDir::new().unwrap();
    assert!(run_suite(&suite, &options(&temp, true)).await.unwrap()[0].passed);
}
#[tokio::test]
async fn anthropic_target() {
    let (url, _) = server(vec![(
        StatusCode::OK,
        json!({"content":[{"text":"ok anthropic"}]}),
    )])
    .await;
    let suite = target_suite("anthropic", &url, 1, "concurrency: 1");
    let temp = TempDir::new().unwrap();
    assert!(run_suite(&suite, &options(&temp, true)).await.unwrap()[0].passed);
}
#[tokio::test]
async fn rate_limit_spaces_requests() {
    let (url, state) = server(vec![(
        StatusCode::OK,
        json!({"choices":[{"message":{"content":"ok"}}]}),
    )])
    .await;
    let suite = target_suite(
        "openai",
        &url,
        3,
        "concurrency: 3\n    requests_per_second: 20",
    );
    let temp = TempDir::new().unwrap();
    run_suite(&suite, &options(&temp, true)).await.unwrap();
    let mut times = state.times.lock().await.clone();
    times.sort();
    assert!(times
        .windows(2)
        .all(|pair| pair[1].duration_since(pair[0]).as_millis() >= 40));
}
#[tokio::test]
async fn retries_429_then_succeeds() {
    let (url, state) = server(vec![
        (StatusCode::TOO_MANY_REQUESTS, json!({})),
        (
            StatusCode::OK,
            json!({"choices":[{"message":{"content":"ok"}}]}),
        ),
    ])
    .await;
    let suite = target_suite("openai", &url, 1, "concurrency: 1\n    retries: 1");
    let temp = TempDir::new().unwrap();
    assert!(run_suite(&suite, &options(&temp, true)).await.unwrap()[0].passed);
    assert_eq!(state.times.lock().await.len(), 2);
}
fn external_suite(kind: &str, url: &str, threshold: f64) -> GateSuite {
    parse_suite(&format!("suite: {{name: ext, pass_threshold: {threshold}}}\ntargets: [{{name: echo, type: echo}}]\ncases:\n- name: ext\n  prompt: answer\n  pass_threshold: {threshold}\n  checks:\n  - kind: {kind}\n    value: expected\n    endpoint: {url}\n    threshold: {threshold}\n")).unwrap()
}
#[tokio::test]
async fn judge_pass_and_fail() {
    let pass = json!({"choices":[{"message":{"content":"{\"pass\":true,\"score\":0.9,\"reason\":\"good\"}"}}]});
    let fail = json!({"choices":[{"message":{"content":"{\"pass\":false,\"score\":0.2,\"reason\":\"bad\"}"}}]});
    let (url, _) = server(vec![(StatusCode::OK, pass), (StatusCode::OK, fail)]).await;
    let temp = TempDir::new().unwrap();
    let mut opts = options(&temp, true);
    assert!(
        run_suite(&external_suite("judge", &url, 0.5), &opts)
            .await
            .unwrap()[0]
            .passed
    );
    opts.no_cache = true;
    assert!(
        !run_suite(&external_suite("judge", &url, 0.5), &opts)
            .await
            .unwrap()[0]
            .passed
    );
}
#[tokio::test]
async fn judge_retries_malformed() {
    let bad = json!({"choices":[{"message":{"content":"not json"}}]});
    let good = json!({"choices":[{"message":{"content":"{\"pass\":true,\"score\":1,\"reason\":\"ok\"}"}}]});
    let (url, state) = server(vec![(StatusCode::OK, bad), (StatusCode::OK, good)]).await;
    let temp = TempDir::new().unwrap();
    assert!(
        run_suite(&external_suite("judge", &url, 0.5), &options(&temp, true))
            .await
            .unwrap()[0]
            .passed
    );
    assert_eq!(state.times.lock().await.len(), 2);
}
#[tokio::test]
async fn semantic_above_and_below() {
    let above = json!({"data":[{"embedding":[1,0]},{"embedding":[0.9,0.1]}]});
    let below = json!({"data":[{"embedding":[1,0]},{"embedding":[0,1]}]});
    let (url, _) = server(vec![(StatusCode::OK, above), (StatusCode::OK, below)]).await;
    let temp = TempDir::new().unwrap();
    assert!(
        run_suite(
            &external_suite("semantic", &url, 0.8),
            &options(&temp, true)
        )
        .await
        .unwrap()[0]
            .passed
    );
    assert!(
        !run_suite(
            &external_suite("semantic", &url, 0.8),
            &options(&temp, true)
        )
        .await
        .unwrap()[0]
            .passed
    );
}
#[tokio::test]
async fn cache_hit_avoids_second_request() {
    let (url, state) = server(vec![(
        StatusCode::OK,
        json!({"choices":[{"message":{"content":"ok"}}]}),
    )])
    .await;
    let suite = target_suite("openai", &url, 1, "concurrency: 1");
    let temp = TempDir::new().unwrap();
    let opts = options(&temp, false);
    run_suite(&suite, &opts).await.unwrap();
    run_suite(&suite, &opts).await.unwrap();
    assert_eq!(state.times.lock().await.len(), 1);
}
