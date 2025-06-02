use crate::{
    flaky::{classify, Stability},
    schema::{Case, CaseResult, CheckResult, GateSuite, RunReport},
};
pub fn score_attempt(case: &Case, checks: Vec<CheckResult>) -> CaseResult {
    let total: f64 = case.checks.iter().map(|c| c.weight).sum();
    let score = case
        .checks
        .iter()
        .zip(&checks)
        .map(|(s, r)| s.weight * r.score)
        .sum::<f64>()
        / total;
    CaseResult {
        name: case.name.clone(),
        passed: score >= case.pass_threshold,
        flaky: false,
        score,
        pass_rate: f64::from(score >= case.pass_threshold),
        checks,
    }
}
pub fn combine_attempts(case: &Case, attempts: Vec<CaseResult>, min: f64, max: f64) -> CaseResult {
    let total = attempts.len();
    let passes = attempts.iter().filter(|a| a.passed).count();
    let rate = passes as f64 / total as f64;
    let mut latest = attempts.into_iter().last().expect("validated repeat count");
    let stability = classify(passes, total, min, max);
    latest.pass_rate = rate;
    latest.flaky = stability == Stability::Flaky;
    latest.passed = stability == Stability::Passing && latest.score >= case.pass_threshold;
    latest
}
pub fn score_suite(suite: &GateSuite, cases: Vec<CaseResult>) -> RunReport {
    let score = cases.iter().map(|c| c.score).sum::<f64>() / cases.len() as f64;
    RunReport {
        suite: suite.suite.name.clone(),
        target: String::new(),
        passed: score >= suite.suite.pass_threshold && cases.iter().all(|c| c.passed),
        score,
        passed_cases: cases.iter().filter(|c| c.passed).count(),
        flaky_cases: cases.iter().filter(|c| c.flaky).count(),
        cases,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use serde_json::Value;
    fn case() -> Case {
        Case {
            name: "c".into(),
            input: Value::Null,
            prompt: "".into(),
            pass_threshold: 0.7,
            checks: vec![
                CheckSpec {
                    kind: "a".into(),
                    value: Value::Null,
                    weight: 3.,
                    threshold: 1.,
                    endpoint: None,
                    model: None,
                },
                CheckSpec {
                    kind: "b".into(),
                    value: Value::Null,
                    weight: 1.,
                    threshold: 1.,
                    endpoint: None,
                    model: None,
                },
            ],
        }
    }
    fn checks(a: bool, b: bool) -> Vec<CheckResult> {
        vec![
            CheckResult {
                kind: "a".into(),
                passed: a,
                score: f64::from(a),
                detail: "".into(),
            },
            CheckResult {
                kind: "b".into(),
                passed: b,
                score: f64::from(b),
                detail: "".into(),
            },
        ]
    }
    #[test]
    fn weighted() {
        let r = score_attempt(&case(), checks(true, false));
        assert_eq!(r.score, 0.75);
        assert!(r.passed);
    }
    #[test]
    fn repeat_flaky() {
        let c = case();
        let a = score_attempt(&c, checks(true, true));
        let b = score_attempt(&c, checks(false, false));
        let r = combine_attempts(&c, vec![a, b], 0.01, 0.99);
        assert!(r.flaky);
        assert_eq!(r.pass_rate, 0.5);
    }
}
