use crate::schema::{CaseResult, RunReport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CheckDiff {
    pub kind: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CaseDiff {
    pub name: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
    pub checks: Vec<CheckDiff>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Comparison {
    pub regression: bool,
    pub score_delta: f64,
    pub regressions: Vec<CaseDiff>,
    pub fixes: Vec<CaseDiff>,
    pub flaky: Vec<String>,
}
pub fn compare_reports(base: &RunReport, current: &RunReport, tolerance: f64) -> Comparison {
    let old: HashMap<&str, &CaseResult> = base.cases.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut regressions = vec![];
    let mut fixes = vec![];
    let mut flaky = vec![];
    for now in &current.cases {
        if now.flaky {
            flaky.push(now.name.clone())
        }
        if let Some(before) = old.get(now.name.as_str()) {
            let delta = now.score - before.score;
            let old_checks: HashMap<&str, f64> = before
                .checks
                .iter()
                .map(|c| (c.kind.as_str(), c.score))
                .collect();
            let checks = now
                .checks
                .iter()
                .filter_map(|c| {
                    old_checks.get(c.kind.as_str()).map(|v| CheckDiff {
                        kind: c.kind.clone(),
                        before: *v,
                        after: c.score,
                        delta: c.score - *v,
                    })
                })
                .collect();
            let diff = CaseDiff {
                name: now.name.clone(),
                before: before.score,
                after: now.score,
                delta,
                checks,
            };
            if delta < -tolerance || (before.passed && !now.passed) {
                regressions.push(diff)
            } else if delta > tolerance || (!before.passed && now.passed) {
                fixes.push(diff)
            }
        }
    }
    Comparison {
        regression: !regressions.is_empty(),
        score_delta: current.score - base.score,
        regressions,
        fixes,
        flaky,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    fn report(score: f64, pass: bool, flaky: bool) -> RunReport {
        RunReport {
            suite: "s".into(),
            target: "t".into(),
            passed: pass,
            score,
            passed_cases: usize::from(pass),
            flaky_cases: usize::from(flaky),
            cases: vec![CaseResult {
                name: "c".into(),
                passed: pass,
                flaky,
                score,
                pass_rate: score,
                checks: vec![CheckResult {
                    kind: "exact".into(),
                    passed: pass,
                    score,
                    detail: "".into(),
                }],
            }],
        }
    }
    #[test]
    fn regression() {
        let c = compare_reports(&report(1., true, false), &report(0.5, false, false), 0.02);
        assert!(c.regression);
        assert_eq!(c.regressions[0].checks[0].delta, -0.5);
    }
    #[test]
    fn fix() {
        assert_eq!(
            compare_reports(&report(0.5, false, false), &report(1., true, false), 0.02)
                .fixes
                .len(),
            1
        );
    }
    #[test]
    fn tolerance() {
        assert!(
            !compare_reports(&report(0.5, true, false), &report(0.49, true, false), 0.02)
                .regression
        );
    }
    #[test]
    fn flaky() {
        assert_eq!(
            compare_reports(&report(1., true, false), &report(1., true, true), 0.02).flaky,
            vec!["c"]
        );
    }
}
