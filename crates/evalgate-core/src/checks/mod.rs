mod json;
mod operational;
mod pattern;
mod similarity;
mod snapshot;
mod text;

use crate::schema::{CheckResult, CheckSpec};

pub trait ExternalChecks {
    fn judge(&self, spec: &CheckSpec, output: &str) -> Result<CheckResult, String>;
    fn semantic(&self, spec: &CheckSpec, output: &str) -> Result<CheckResult, String>;
}
pub struct CheckContext<'a> {
    pub output: &'a str,
    pub latency_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub snapshot: Option<&'a str>,
    pub external: Option<&'a dyn ExternalChecks>,
}
impl<'a> CheckContext<'a> {
    pub fn text(output: &'a str) -> Self {
        Self {
            output,
            latency_ms: None,
            cost_usd: None,
            snapshot: None,
            external: None,
        }
    }
}
pub fn evaluate(spec: &CheckSpec, ctx: &CheckContext<'_>) -> Result<CheckResult, String> {
    match spec.kind.as_str() {
        "exact" | "includes" | "starts_with" | "ends_with" => text::run(spec, ctx.output),
        "matches" | "not_matches" => pattern::run(spec, ctx.output),
        "json_valid" | "json_shape" => json::run(spec, ctx.output),
        "edit_distance" | "ngram_overlap" => similarity::run(spec, ctx.output),
        "max_latency_ms" | "max_cost_usd" => operational::run(spec, ctx),
        "snapshot" => snapshot::run(spec, ctx),
        "judge" => ctx
            .external
            .ok_or("judge check requires an external check provider")?
            .judge(spec, ctx.output),
        "semantic" => ctx
            .external
            .ok_or("semantic check requires an external check provider")?
            .semantic(spec, ctx.output),
        other => Err(format!("unknown check kind {other:?}")),
    }
}
pub(crate) fn result(
    kind: &str,
    passed: bool,
    score: f64,
    detail: impl Into<String>,
) -> CheckResult {
    CheckResult {
        kind: kind.into(),
        passed,
        score: score.clamp(0.0, 1.0),
        detail: detail.into(),
    }
}
