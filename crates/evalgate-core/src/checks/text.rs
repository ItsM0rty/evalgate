use super::result;
use crate::schema::{CheckResult, CheckSpec};
pub fn run(spec: &CheckSpec, output: &str) -> Result<CheckResult, String> {
    let expected = spec
        .value
        .as_str()
        .ok_or_else(|| format!("{} value must be a string", spec.kind))?;
    let passed = match spec.kind.as_str() {
        "exact" => output == expected,
        "includes" => output.contains(expected),
        "starts_with" => output.starts_with(expected),
        "ends_with" => output.ends_with(expected),
        _ => unreachable!(),
    };
    Ok(result(
        &spec.kind,
        passed,
        f64::from(passed),
        format!("expected {:?}", expected),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn spec(kind: &str, value: &str) -> CheckSpec {
        CheckSpec {
            kind: kind.into(),
            value: json!(value),
            weight: 1.0,
            threshold: 1.0,
            endpoint: None,
            model: None,
        }
    }
    #[test]
    fn exact() {
        assert!(run(&spec("exact", "x"), "x").unwrap().passed);
    }
    #[test]
    fn includes() {
        assert!(run(&spec("includes", "llo"), "hello").unwrap().passed);
    }
    #[test]
    fn starts_and_ends() {
        assert!(
            run(&spec("starts_with", "a"), "abc").unwrap().passed
                && run(&spec("ends_with", "c"), "abc").unwrap().passed
        );
    }
    #[test]
    fn requires_string() {
        let mut s = spec("exact", "x");
        s.value = json!(1);
        assert!(run(&s, "1").is_err());
    }
}
