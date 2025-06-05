use crate::{compare::Comparison, schema::RunReport};
pub fn terminal(report: &RunReport) -> String {
    let mut out = format!("{:<28} {:>7} {:>8}\n", "CASE", "SCORE", "VERDICT");
    for c in &report.cases {
        out += &format!(
            "{:<28} {:>6.1}% {:>8}\n",
            c.name,
            c.score * 100.,
            if c.flaky {
                "FLAKY"
            } else if c.passed {
                "PASS"
            } else {
                "FAIL"
            }
        );
    }
    out += &format!(
        "\nSuite score: {:0.1}% [{}]\n",
        report.score * 100.,
        if report.passed { "PASS" } else { "FAIL" }
    );
    out
}
pub fn json(report: &RunReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| e.to_string())
}
pub fn junit(report: &RunReport) -> String {
    let failures = report.cases.iter().filter(|c| !c.passed).count();
    let mut s=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\">",xml(&report.suite),report.cases.len(),failures);
    for c in &report.cases {
        s += &format!("\n  <testcase name=\"{}\">", xml(&c.name));
        if !c.passed {
            s += &format!("<failure message=\"score {:.4}\" />", c.score)
        }
        s += "</testcase>";
    }
    s += "\n</testsuite>\n";
    s
}
pub fn markdown_comparison(c: &Comparison) -> String {
    let mut s = format!(
        "# Evalgate comparison\n\nScore delta: {:+.4}\n\n## Regressions\n\n",
        c.score_delta
    );
    if c.regressions.is_empty() {
        s += "None.\n"
    } else {
        for d in &c.regressions {
            s += &format!(
                "- {}: {:.4} to {:.4} ({:+.4})\n",
                d.name, d.before, d.after, d.delta
            )
        }
    }
    s += "\n## Fixes\n\n";
    if c.fixes.is_empty() {
        s += "None.\n"
    } else {
        for d in &c.fixes {
            s += &format!(
                "- {}: {:.4} to {:.4} ({:+.4})\n",
                d.name, d.before, d.after, d.delta
            )
        }
    }
    s += "\n## Flaky cases\n\n";
    if c.flaky.is_empty() {
        s += "None.\n"
    } else {
        for n in &c.flaky {
            s += &format!("- {n}\n")
        }
    }
    s
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compare::*, schema::*};
    fn r() -> RunReport {
        RunReport {
            suite: "A & B".into(),
            target: "t".into(),
            passed: false,
            score: 0.5,
            passed_cases: 0,
            flaky_cases: 0,
            cases: vec![CaseResult {
                name: "case <1>".into(),
                passed: false,
                flaky: false,
                score: 0.5,
                pass_rate: 0.,
                checks: vec![],
            }],
        }
    }
    #[test]
    fn table() {
        assert!(terminal(&r()).contains("Suite score: 50.0% [FAIL]"))
    }
    #[test]
    fn json_format() {
        assert!(json(&r()).unwrap().contains("\"score\": 0.5"))
    }
    #[test]
    fn junit_snapshot() {
        assert_eq!(junit(&r()),"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"A &amp; B\" tests=\"1\" failures=\"1\">\n  <testcase name=\"case &lt;1&gt;\"><failure message=\"score 0.5000\" /></testcase>\n</testsuite>\n")
    }
    #[test]
    fn markdown_snapshot() {
        let c = Comparison {
            regression: false,
            score_delta: 0.,
            regressions: vec![],
            fixes: vec![],
            flaky: vec![],
        };
        assert_eq!(markdown_comparison(&c),"# Evalgate comparison\n\nScore delta: +0.0000\n\n## Regressions\n\nNone.\n\n## Fixes\n\nNone.\n\n## Flaky cases\n\nNone.\n")
    }
}
