use anyhow::{anyhow, Context, Result};
use evalgate_core::{
    checks::{self, CheckContext},
    schema::{Case, CheckResult, CheckSpec, GateSuite, RunReport, Target, TargetKind},
    score, template,
};
use futures::{stream::FuturesUnordered, StreamExt};
use rand::Rng;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetOutput {
    pub output: String,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}
#[derive(Clone)]
pub struct RunOptions {
    pub repeat: usize,
    pub no_cache: bool,
    pub update_snapshots: bool,
    pub root: PathBuf,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            repeat: 1,
            no_cache: false,
            update_snapshots: false,
            root: PathBuf::from("."),
        }
    }
}
#[derive(Clone)]
struct RateGate {
    interval: Option<Duration>,
    next: Arc<Mutex<Instant>>,
}
impl RateGate {
    fn new(rps: Option<f64>) -> Self {
        Self {
            interval: rps.map(|v| Duration::from_secs_f64(1.0 / v)),
            next: Arc::new(Mutex::new(Instant::now())),
        }
    }
    async fn wait(&self) {
        let Some(interval) = self.interval else {
            return;
        };
        let mut next = self.next.lock().await;
        let now = Instant::now();
        if *next > now {
            tokio::time::sleep(*next - now).await;
        }
        *next = Instant::now() + interval;
    }
}
pub async fn run_suite(suite: &GateSuite, options: &RunOptions) -> Result<Vec<RunReport>> {
    if options.repeat == 0 {
        return Err(anyhow!("repeat must be at least 1"));
    }
    let mut reports = Vec::new();
    let targets = if suite.targets.is_empty() {
        vec![Target {
            name: "echo".into(),
            kind: TargetKind::Echo,
            endpoint: None,
            api_key_env: None,
            model: None,
            file: None,
            params: Value::Null,
            concurrency: 4,
            requests_per_second: None,
            retries: 0,
            timeout_ms: 30_000,
        }]
    } else {
        suite.targets.clone()
    };
    for target in targets {
        let mut attempts: Vec<Vec<_>> = (0..suite.cases.len()).map(|_| Vec::new()).collect();
        for _ in 0..options.repeat {
            for (index, result) in execute_target(suite, &target, options)
                .await?
                .into_iter()
                .enumerate()
            {
                attempts[index].push(result);
            }
        }
        let cases = suite
            .cases
            .iter()
            .zip(attempts)
            .map(|(case, items)| {
                score::combine_attempts(
                    case,
                    items,
                    suite.suite.flaky_min_rate,
                    suite.suite.flaky_max_rate,
                )
            })
            .collect();
        let mut report = score::score_suite(suite, cases);
        report.target = target.name.clone();
        reports.push(report);
    }
    Ok(reports)
}
async fn execute_target(
    suite: &GateSuite,
    target: &Target,
    options: &RunOptions,
) -> Result<Vec<evalgate_core::CaseResult>> {
    let client = Client::builder()
        .timeout(Duration::from_millis(target.timeout_ms))
        .build()?;
    let semaphore = Arc::new(Semaphore::new(target.concurrency));
    let gate = RateGate::new(target.requests_per_second);
    let replay = if matches!(target.kind, TargetKind::Replay) {
        load_replay(&options.root.join(target.file.as_ref().expect("validated"))).await?
    } else {
        HashMap::new()
    };
    let mut tasks = FuturesUnordered::new();
    for (index, case) in suite.cases.iter().cloned().enumerate() {
        let target = target.clone();
        let client = client.clone();
        let permit = semaphore.clone();
        let gate = gate.clone();
        let options = options.clone();
        let recorded = replay.get(&case.name).cloned();
        tasks.push(async move {
            let _permit = permit.acquire_owned().await?;
            let output = execute_case(&client, &target, &case, recorded, gate, &options).await?;
            let checks = evaluate_checks(&client, &case, &output, &options).await?;
            Ok::<_, anyhow::Error>((index, score::score_attempt(&case, checks)))
        });
    }
    let mut results = vec![None; suite.cases.len()];
    while let Some(item) = tasks.next().await {
        let (index, result) = item?;
        results[index] = Some(result);
    }
    results
        .into_iter()
        .map(|v| v.ok_or_else(|| anyhow!("missing case result")))
        .collect()
}
async fn execute_case(
    client: &Client,
    target: &Target,
    case: &Case,
    replay: Option<TargetOutput>,
    gate: RateGate,
    options: &RunOptions,
) -> Result<TargetOutput> {
    let prompt = template::render(&case.prompt, &case.input).map_err(anyhow::Error::msg)?;
    match target.kind {
        TargetKind::Echo => Ok(TargetOutput {
            output: prompt,
            latency_ms: Some(0),
            cost_usd: Some(0.0),
        }),
        TargetKind::Replay => {
            replay.ok_or_else(|| anyhow!("replay has no output for case {:?}", case.name))
        }
        TargetKind::Openai | TargetKind::Anthropic => {
            let key = cache_key(&json!({"target":target,"prompt":prompt}));
            if !options.no_cache {
                if let Some(v) = cache_read(&options.root, &key).await? {
                    return Ok(v);
                }
            }
            let start = Instant::now();
            let value = retry_request(client, target, &prompt, &gate).await?;
            let output = match target.kind {
                TargetKind::Openai => value
                    .pointer("/choices/0/message/content")
                    .and_then(Value::as_str),
                TargetKind::Anthropic => value.pointer("/content/0/text").and_then(Value::as_str),
                _ => None,
            }
            .ok_or_else(|| anyhow!("target response did not contain text"))?
            .to_owned();
            let result = TargetOutput {
                output,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                cost_usd: None,
            };
            if !options.no_cache {
                cache_write(&options.root, &key, &result).await?;
            }
            Ok(result)
        }
    }
}
async fn retry_request(
    client: &Client,
    target: &Target,
    prompt: &str,
    gate: &RateGate,
) -> Result<Value> {
    let endpoint = target.endpoint.as_deref().expect("validated");
    for attempt in 0..=target.retries {
        gate.wait().await;
        let body = match target.kind {
            TargetKind::Openai => {
                json!({"model":target.model.as_deref().unwrap_or("default"),"messages":[{"role":"user","content":prompt}],"params":target.params})
            }
            TargetKind::Anthropic => {
                json!({"model":target.model.as_deref().unwrap_or("default"),"max_tokens":1024,"messages":[{"role":"user","content":prompt}],"params":target.params})
            }
            _ => unreachable!(),
        };
        let mut request = client.post(endpoint).json(&body);
        if let Some(env) = &target.api_key_env {
            request = request.bearer_auth(
                std::env::var(env)
                    .with_context(|| format!("environment variable {env} is not set"))?,
            );
        }
        if matches!(target.kind, TargetKind::Anthropic) {
            request = request.header("anthropic-version", "2023-06-01");
        }
        let response = request.send().await?;
        if response.status().is_success() {
            return Ok(response.json().await?);
        }
        let status = response.status();
        if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
            || attempt == target.retries
        {
            return Err(anyhow!("target request failed with HTTP {status}"));
        }
        let base = 50_u64.saturating_mul(1_u64 << attempt.min(10));
        let jitter = rand::rng().random_range(0..=base / 2);
        tokio::time::sleep(Duration::from_millis(base + jitter)).await;
    }
    unreachable!()
}
async fn evaluate_checks(
    client: &Client,
    case: &Case,
    output: &TargetOutput,
    options: &RunOptions,
) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();
    for spec in &case.checks {
        let result = match spec.kind.as_str() {
            "judge" => judge(client, spec, &output.output, options).await?,
            "semantic" => semantic(client, spec, &output.output, options).await?,
            "snapshot" => {
                let path = snapshot_path(&options.root, &case.name);
                if options.update_snapshots {
                    if let Some(parent) = path.parent() {
                        tokio::fs::create_dir_all(parent).await?
                    }
                    tokio::fs::write(&path, &output.output).await?;
                }
                let snapshot = tokio::fs::read_to_string(&path).await.ok();
                let mut ctx = CheckContext::text(&output.output);
                ctx.snapshot = snapshot.as_deref();
                checks::evaluate(spec, &ctx).map_err(anyhow::Error::msg)?
            }
            _ => {
                let mut ctx = CheckContext::text(&output.output);
                ctx.latency_ms = output.latency_ms;
                ctx.cost_usd = output.cost_usd;
                checks::evaluate(spec, &ctx).map_err(anyhow::Error::msg)?
            }
        };
        results.push(result);
    }
    Ok(results)
}
async fn judge(
    client: &Client,
    spec: &CheckSpec,
    output: &str,
    options: &RunOptions,
) -> Result<CheckResult> {
    let endpoint = spec
        .endpoint
        .as_deref()
        .ok_or_else(|| anyhow!("judge endpoint is required"))?;
    let key = cache_key(&json!({"judge":spec,"output":output}));
    if !options.no_cache {
        if let Some(v) = cache_read_value(&options.root, &key).await? {
            return Ok(serde_json::from_value(v)?);
        }
    }
    let prompt=format!("Grade this output against the criterion. Return only JSON with pass, score, reason.\nCriterion: {}\nOutput: {}",spec.value,output);
    for attempt in 0..2 {
        let value:Value=client.post(endpoint).json(&json!({"model":spec.model.as_deref().unwrap_or("default"),"messages":[{"role":"user","content":prompt}]})).send().await?.error_for_status()?.json().await?;
        if let Some(text) = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
        {
            if let Ok(grade) = serde_json::from_str::<JudgeGrade>(text) {
                if grade.score.is_finite() && (0.0..=1.0).contains(&grade.score) {
                    let result = CheckResult {
                        kind: "judge".into(),
                        passed: grade.pass && grade.score >= spec.threshold,
                        score: grade.score,
                        detail: grade.reason,
                    };
                    if !options.no_cache {
                        cache_write_value(&options.root, &key, &serde_json::to_value(&result)?)
                            .await?
                    }
                    return Ok(result);
                }
            }
        }
        if attempt == 1 {
            return Err(anyhow!("judge returned malformed grading JSON twice"));
        }
    }
    unreachable!()
}
#[derive(Deserialize)]
struct JudgeGrade {
    pass: bool,
    score: f64,
    reason: String,
}
async fn semantic(
    client: &Client,
    spec: &CheckSpec,
    output: &str,
    options: &RunOptions,
) -> Result<CheckResult> {
    let endpoint = spec
        .endpoint
        .as_deref()
        .ok_or_else(|| anyhow!("semantic endpoint is required"))?;
    let expected = spec
        .value
        .as_str()
        .ok_or_else(|| anyhow!("semantic value must be a string"))?;
    let key = cache_key(&json!({"semantic":spec,"output":output}));
    if !options.no_cache {
        if let Some(v) = cache_read_value(&options.root, &key).await? {
            return Ok(serde_json::from_value(v)?);
        }
    }
    let value: Value = client
        .post(endpoint)
        .json(
            &json!({"model":spec.model.as_deref().unwrap_or("default"),"input":[output,expected]}),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("embedding response has no data array"))?;
    let vectors = data
        .iter()
        .map(|v| {
            v.get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("embedding is missing"))
                .and_then(|a| {
                    a.iter()
                        .map(|n| {
                            n.as_f64()
                                .ok_or_else(|| anyhow!("embedding value is not numeric"))
                        })
                        .collect::<Result<Vec<_>>>()
                })
        })
        .collect::<Result<Vec<_>>>()?;
    if vectors.len() != 2 {
        return Err(anyhow!("embedding response must contain two vectors"));
    }
    let score = cosine(&vectors[0], &vectors[1])?;
    let result = CheckResult {
        kind: "semantic".into(),
        passed: score >= spec.threshold,
        score,
        detail: format!("cosine {score:.4}, threshold {:.4}", spec.threshold),
    };
    if !options.no_cache {
        cache_write_value(&options.root, &key, &serde_json::to_value(&result)?).await?
    }
    Ok(result)
}
fn cosine(a: &[f64], b: &[f64]) -> Result<f64> {
    if a.len() != b.len() || a.is_empty() {
        return Err(anyhow!("embedding dimensions differ"));
    }
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0. || nb == 0. {
        return Err(anyhow!("embedding has zero magnitude"));
    }
    Ok((dot / (na * nb)).clamp(0., 1.))
}
async fn load_replay(path: &Path) -> Result<HashMap<String, TargetOutput>> {
    let text = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read replay {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let value: Value =
                serde_json::from_str(line).with_context(|| format!("replay line {}", i + 1))?;
            let name = value
                .get("case")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("replay line {} has no case", i + 1))?
                .to_owned();
            let output: TargetOutput = serde_json::from_value(value)?;
            Ok((name, output))
        })
        .collect()
}
fn snapshot_path(root: &Path, name: &str) -> PathBuf {
    let safe = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    root.join(".evalgate/snapshots")
        .join(format!("{safe}.snap"))
}
fn cache_key(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable"))
    )
}
fn cache_path(root: &Path, key: &str) -> PathBuf {
    root.join(".evalgate/cache").join(format!("{key}.json"))
}
async fn cache_read(root: &Path, key: &str) -> Result<Option<TargetOutput>> {
    cache_read_value(root, key)
        .await?
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
async fn cache_write(root: &Path, key: &str, value: &TargetOutput) -> Result<()> {
    cache_write_value(root, key, &serde_json::to_value(value)?).await
}
async fn cache_read_value(root: &Path, key: &str) -> Result<Option<Value>> {
    match tokio::fs::read(cache_path(root, key)).await {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
async fn cache_write_value(root: &Path, key: &str, value: &Value) -> Result<()> {
    let path = cache_path(root, key);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?
    }
    tokio::fs::write(path, serde_json::to_vec(value)?).await?;
    Ok(())
}
