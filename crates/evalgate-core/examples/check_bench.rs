use evalgate_core::{
    checks::{evaluate, CheckContext},
    schema::CheckSpec,
};
use serde_json::json;
use std::time::Instant;

fn main() {
    let spec = CheckSpec {
        kind: "includes".into(),
        value: json!("ORD-12345"),
        weight: 1.0,
        threshold: 1.0,
        endpoint: None,
        model: None,
    };
    let started = Instant::now();
    let passed = (0..10_000)
        .filter(|_| {
            evaluate(&spec, &CheckContext::text("Order ORD-12345 has shipped."))
                .expect("valid check")
                .passed
        })
        .count();
    println!(
        "{passed} checks passed in {:.3} ms",
        started.elapsed().as_secs_f64() * 1_000.0
    );
}
