use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn evaluate(suite_yaml: &str, outputs_json: &str) -> Result<String, JsValue> {
    evalgate_core::evaluate(suite_yaml, outputs_json)
        .and_then(|report| serde_json::to_string(&report).map_err(|error| error.to_string()))
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn compare(baseline_json: &str, current_json: &str, tolerance: f64) -> Result<String, JsValue> {
    let baseline = serde_json::from_str(baseline_json)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let current = serde_json::from_str(current_json)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    serde_json::to_string(&evalgate_core::compare_reports(
        &baseline, &current, tolerance,
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn render_markdown(comparison_json: &str) -> Result<String, JsValue> {
    let comparison = serde_json::from_str(comparison_json)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(evalgate_core::report::markdown_comparison(&comparison))
}

#[wasm_bindgen]
pub fn render_junit(report_json: &str) -> Result<String, JsValue> {
    let report =
        serde_json::from_str(report_json).map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(evalgate_core::report::junit(&report))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUITE: &str = "suite:\n  name: Demo\n  pass_threshold: 0.5\ncases:\n  - name: reply\n    checks:\n      - { kind: exact, value: ok }\n";

    #[test]
    fn evaluate_export_scores_repeated_outputs() {
        let report: evalgate_core::RunReport =
            serde_json::from_str(&evaluate(SUITE, r#"[["ok","no","ok"]]"#).unwrap()).unwrap();
        assert_eq!(report.cases[0].pass_rate, 2.0 / 3.0);
        assert!(report.cases[0].flaky);
    }

    #[test]
    fn compare_export_reports_regression() {
        let baseline = evaluate(SUITE, r#"["ok"]"#).unwrap();
        let current = evaluate(SUITE, r#"["no"]"#).unwrap();
        let comparison: evalgate_core::Comparison =
            serde_json::from_str(&compare(&baseline, &current, 0.02).unwrap()).unwrap();
        assert!(comparison.regression);
    }

    #[test]
    fn render_markdown_export_uses_core_report() {
        let baseline = evaluate(SUITE, r#"["ok"]"#).unwrap();
        let current = evaluate(SUITE, r#"["no"]"#).unwrap();
        let comparison = compare(&baseline, &current, 0.02).unwrap();
        assert!(render_markdown(&comparison)
            .unwrap()
            .contains("## Regressions"));
    }

    #[test]
    fn render_junit_export_uses_core_report() {
        let report = evaluate(SUITE, r#"["no"]"#).unwrap();
        let junit = render_junit(&report).unwrap();
        assert!(junit.contains(r#"<testsuite name="Demo" tests="1" failures="1">"#));
    }
}
