use super::{result, CheckContext};
use crate::schema::{CheckResult, CheckSpec};
pub fn run(spec: &CheckSpec, ctx: &CheckContext<'_>) -> Result<CheckResult, String> {
    let limit = spec
        .value
        .as_f64()
        .ok_or("operational check value must be a number")?;
    let actual = if spec.kind == "max_latency_ms" {
        ctx.latency_ms
            .map(|v| v as f64)
            .ok_or("latency is unavailable")?
    } else {
        ctx.cost_usd.ok_or("cost is unavailable")?
    };
    let passed = actual <= limit;
    Ok(result(
        &spec.kind,
        passed,
        f64::from(passed),
        format!("actual {actual:.6}, maximum {limit:.6}"),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn s(k: &str, v: f64) -> CheckSpec {
        CheckSpec {
            kind: k.into(),
            value: json!(v),
            weight: 1.,
            threshold: 1.,
            endpoint: None,
            model: None,
        }
    }
    #[test]
    fn latency() {
        let mut c = CheckContext::text("");
        c.latency_ms = Some(9);
        assert!(run(&s("max_latency_ms", 10.), &c).unwrap().passed);
    }
    #[test]
    fn cost() {
        let mut c = CheckContext::text("");
        c.cost_usd = Some(0.2);
        assert!(!run(&s("max_cost_usd", 0.1), &c).unwrap().passed);
    }
    #[test]
    fn missing() {
        assert!(run(&s("max_latency_ms", 1.), &CheckContext::text("")).is_err());
    }
}
