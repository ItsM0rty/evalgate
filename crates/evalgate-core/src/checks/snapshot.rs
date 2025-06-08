use super::{result, CheckContext};
use crate::schema::{CheckResult, CheckSpec};
pub fn run(spec: &CheckSpec, ctx: &CheckContext<'_>) -> Result<CheckResult, String> {
    let expected = ctx
        .snapshot
        .or_else(|| spec.value.as_str())
        .ok_or("snapshot is unavailable")?;
    let passed = ctx.output == expected;
    let detail = if passed {
        "snapshot matched".into()
    } else {
        first_difference(expected, ctx.output)
    };
    Ok(result("snapshot", passed, f64::from(passed), detail))
}
fn first_difference(a: &str, b: &str) -> String {
    let line = a
        .lines()
        .zip(b.lines())
        .position(|(x, y)| x != y)
        .map_or(a.lines().count().min(b.lines().count()) + 1, |i| i + 1);
    format!("snapshot differs at line {line}")
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn s() -> CheckSpec {
        CheckSpec {
            kind: "snapshot".into(),
            value: Value::Null,
            weight: 1.,
            threshold: 1.,
            endpoint: None,
            model: None,
        }
    }
    #[test]
    fn match_snapshot() {
        let mut c = CheckContext::text("x");
        c.snapshot = Some("x");
        assert!(run(&s(), &c).unwrap().passed);
    }
    #[test]
    fn diff_snapshot() {
        let mut c = CheckContext::text("b");
        c.snapshot = Some("a");
        assert!(run(&s(), &c).unwrap().detail.contains("line 1"));
    }
}
