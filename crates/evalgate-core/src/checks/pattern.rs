use super::result;
use crate::schema::{CheckResult, CheckSpec};
use regex::Regex;
pub fn run(spec: &CheckSpec, output: &str) -> Result<CheckResult, String> {
    let value = spec
        .value
        .as_str()
        .ok_or("pattern value must be a string")?;
    let matched = Regex::new(value)
        .map_err(|e| format!("invalid regex: {e}"))?
        .is_match(output);
    let passed = if spec.kind == "not_matches" {
        !matched
    } else {
        matched
    };
    Ok(result(
        &spec.kind,
        passed,
        f64::from(passed),
        format!("pattern {:?}", value),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn s(kind: &str, v: &str) -> CheckSpec {
        CheckSpec {
            kind: kind.into(),
            value: json!(v),
            weight: 1.,
            threshold: 1.,
            endpoint: None,
            model: None,
        }
    }
    #[test]
    fn matches() {
        assert!(run(&s("matches", r"^\d+$"), "42").unwrap().passed);
    }
    #[test]
    fn negative() {
        assert!(run(&s("not_matches", "bad"), "good").unwrap().passed);
    }
    #[test]
    fn invalid() {
        assert!(run(&s("matches", "["), "x").is_err());
    }
}
