pub mod checks;
pub mod compare;
pub mod flaky;
pub mod report;
pub mod schema;
pub mod score;
pub mod template;

pub use compare::{compare_reports, Comparison};
pub use schema::{CaseResult, CheckResult, GateSuite, RunReport};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum CaseOutputs {
    Single(String),
    Samples(Vec<String>),
}

pub fn parse_suite(source: &str) -> Result<GateSuite, String> {
    schema::parse_and_validate(source)
}

pub fn evaluate(source: &str, outputs: &str) -> Result<RunReport, String> {
    let suite = parse_suite(source)?;
    let outputs: Vec<CaseOutputs> = serde_json::from_str(outputs).map_err(|e| e.to_string())?;
    if outputs.len() != suite.cases.len() {
        return Err(format!(
            "expected {} outputs, received {}",
            suite.cases.len(),
            outputs.len()
        ));
    }
    let cases = suite
        .cases
        .iter()
        .zip(outputs)
        .map(|(case, outputs)| {
            let samples = match outputs {
                CaseOutputs::Single(output) => vec![output],
                CaseOutputs::Samples(samples) if samples.is_empty() => {
                    return Err(format!("case {:?} has no output samples", case.name));
                }
                CaseOutputs::Samples(samples) => samples,
            };
            let attempts = samples
                .into_iter()
                .map(|output| {
                    let results = case
                        .checks
                        .iter()
                        .map(|spec| checks::evaluate(spec, &checks::CheckContext::text(&output)))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(score::score_attempt(case, results))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(score::combine_attempts(
                case,
                attempts,
                suite.suite.flaky_min_rate,
                suite.suite.flaky_max_rate,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(score::score_suite(&suite, cases))
}
