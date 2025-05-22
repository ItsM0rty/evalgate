use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

fn one() -> f64 {
    1.0
}
fn default_concurrency() -> usize {
    4
}
fn default_retries() -> usize {
    2
}
fn default_timeout() -> u64 {
    30_000
}
fn default_flaky_min() -> f64 {
    0.01
}
fn default_flaky_max() -> f64 {
    0.99
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GateSuite {
    pub suite: SuiteSettings,
    #[serde(default)]
    pub targets: Vec<Target>,
    pub cases: Vec<Case>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SuiteSettings {
    pub name: String,
    #[serde(default = "one")]
    pub pass_threshold: f64,
    #[serde(default = "default_flaky_min")]
    pub flaky_min_rate: f64,
    #[serde(default = "default_flaky_max")]
    pub flaky_max_rate: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Openai,
    Anthropic,
    Echo,
    Replay,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: TargetKind,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default)]
    pub requests_per_second: Option<f64>,
    #[serde(default = "default_retries")]
    pub retries: usize,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Case {
    pub name: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "one")]
    pub pass_threshold: f64,
    pub checks: Vec<CheckSpec>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckSpec {
    pub kind: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default = "one")]
    pub weight: f64,
    #[serde(default = "one")]
    pub threshold: f64,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CheckResult {
    pub kind: String,
    pub passed: bool,
    pub score: f64,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CaseResult {
    pub name: String,
    pub passed: bool,
    pub flaky: bool,
    pub score: f64,
    pub pass_rate: f64,
    pub checks: Vec<CheckResult>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    pub suite: String,
    pub target: String,
    pub passed: bool,
    pub score: f64,
    pub passed_cases: usize,
    pub flaky_cases: usize,
    pub cases: Vec<CaseResult>,
}

pub fn parse_and_validate(source: &str) -> Result<GateSuite, String> {
    let suite: GateSuite = serde_yaml::from_str(source).map_err(|e| {
        format!(
            "suite YAML at line {}: {}",
            e.location().map_or(1, |l| l.line()),
            e
        )
    })?;
    validate(&suite).map_err(|error| {
        let line = validation_line(source, &error);
        format!("suite validation near line {line}: {error}")
    })?;
    Ok(suite)
}
fn validation_line(source: &str, error: &str) -> usize {
    let quoted = error.split('"').nth(1);
    source
        .lines()
        .position(|line| quoted.is_some_and(|value| line.contains(value)))
        .or_else(|| {
            [
                "pass_threshold",
                "flaky_min_rate",
                "flaky_max_rate",
                "targets",
                "cases",
                "checks",
            ]
            .iter()
            .find_map(|key| {
                error
                    .contains(key)
                    .then(|| source.lines().position(|line| line.contains(key)))
                    .flatten()
            })
        })
        .map_or(1, |line| line + 1)
}
pub fn validate(suite: &GateSuite) -> Result<(), String> {
    if suite.suite.name.trim().is_empty() {
        return Err("suite.name must not be empty".into());
    }
    unit("suite.pass_threshold", suite.suite.pass_threshold)?;
    unit("suite.flaky_min_rate", suite.suite.flaky_min_rate)?;
    unit("suite.flaky_max_rate", suite.suite.flaky_max_rate)?;
    if suite.suite.flaky_min_rate >= suite.suite.flaky_max_rate {
        return Err("suite.flaky_min_rate must be less than flaky_max_rate".into());
    }
    if suite.cases.is_empty() {
        return Err("cases must contain at least one case".into());
    }
    let mut names = HashSet::new();
    for target in &suite.targets {
        if !names.insert(&target.name) {
            return Err(format!("target name {:?} is duplicated", target.name));
        }
        if target.concurrency == 0 {
            return Err(format!(
                "target {:?}: concurrency must be at least 1",
                target.name
            ));
        }
        if target
            .requests_per_second
            .is_some_and(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(format!(
                "target {:?}: requests_per_second must be positive",
                target.name
            ));
        }
        match target.kind {
            TargetKind::Openai | TargetKind::Anthropic if target.endpoint.is_none() => {
                return Err(format!("target {:?}: endpoint is required", target.name));
            }
            TargetKind::Replay if target.file.is_none() => {
                return Err(format!("target {:?}: file is required", target.name));
            }
            _ => {}
        }
    }
    names.clear();
    for case in &suite.cases {
        if case.name.trim().is_empty() {
            return Err("case.name must not be empty".into());
        }
        if !names.insert(&case.name) {
            return Err(format!("case name {:?} is duplicated", case.name));
        }
        unit(
            &format!("case {:?} pass_threshold", case.name),
            case.pass_threshold,
        )?;
        if case.checks.is_empty() {
            return Err(format!("case {:?}: checks must not be empty", case.name));
        }
        if !case.input.is_object() && !case.input.is_null() {
            return Err(format!("case {:?}: input must be a map", case.name));
        }
        for check in &case.checks {
            if check.weight <= 0.0 || !check.weight.is_finite() {
                return Err(format!(
                    "case {:?}: check weight must be positive",
                    case.name
                ));
            }
            unit(
                &format!("case {:?} check threshold", case.name),
                check.threshold,
            )?;
        }
    }
    Ok(())
}
fn unit(name: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0 and 1"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const OK: &str = "suite: {name: x}\ncases:\n- name: c\n  input: {x: y}\n  checks: [{kind: includes, value: y}]\n";
    #[test]
    fn parses_valid() {
        assert_eq!(parse_and_validate(OK).unwrap().cases.len(), 1);
    }
    #[test]
    fn rejects_empty_cases() {
        assert!(parse_and_validate("suite: {name: x}\ncases: []")
            .unwrap_err()
            .contains("at least"));
    }
    #[test]
    fn rejects_bad_threshold() {
        assert!(parse_and_validate(
            "suite: {name: x, pass_threshold: 2}\ncases: [{name: c, checks: [{kind: exact}]}]"
        )
        .unwrap_err()
        .contains("line 1"));
    }
    #[test]
    fn rejects_duplicate_cases() {
        assert!(parse_and_validate("suite: {name: x}\ncases: [{name: c, checks: [{kind: exact}]}, {name: c, checks: [{kind: exact}]}]").unwrap_err().contains("duplicated"));
    }
    #[test]
    fn rejects_bad_yaml_with_line() {
        assert!(parse_and_validate("suite: [\n")
            .unwrap_err()
            .contains("line"));
    }
    #[test]
    fn replay_needs_file() {
        assert!(parse_and_validate("suite: {name: x}\ntargets: [{name: r, type: replay}]\ncases: [{name: c, checks: [{kind: exact}]}]").unwrap_err().contains("file"));
    }
}
