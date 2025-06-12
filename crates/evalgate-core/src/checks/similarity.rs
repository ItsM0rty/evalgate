use super::result;
use crate::schema::{CheckResult, CheckSpec};
use std::collections::HashSet;
pub fn run(spec: &CheckSpec, output: &str) -> Result<CheckResult, String> {
    let expected = spec
        .value
        .as_str()
        .ok_or("similarity value must be a string")?;
    let score = if spec.kind == "edit_distance" {
        edit_similarity(output, expected)
    } else {
        ngram_overlap(output, expected, 2)
    };
    Ok(result(
        &spec.kind,
        score >= spec.threshold,
        score,
        format!("similarity {score:.4}, threshold {:.4}", spec.threshold),
    ))
}
pub fn edit_similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ac) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(prev + usize::from(ac != bc));
            prev = old;
        }
    }
    let max = a.len().max(b.len());
    if max == 0 {
        1.0
    } else {
        1.0 - row[b.len()] as f64 / max as f64
    }
}
pub fn ngram_overlap(a: &str, b: &str, n: usize) -> f64 {
    let grams = |s: &str| -> HashSet<String> {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < n {
            return [s.to_owned()].into_iter().collect();
        }
        chars.windows(n).map(|w| w.iter().collect()).collect()
    };
    let x = grams(a);
    let y = grams(b);
    let union = x.union(&y).count();
    if union == 0 {
        1.0
    } else {
        x.intersection(&y).count() as f64 / union as f64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edit_identical() {
        assert_eq!(edit_similarity("abc", "abc"), 1.);
    }
    #[test]
    fn edit_empty() {
        assert_eq!(edit_similarity("", ""), 1.);
        assert_eq!(edit_similarity("a", ""), 0.);
    }
    #[test]
    fn edit_unicode() {
        assert!((edit_similarity("cafe", "cafe") - 1.).abs() < 1e-9);
    }
    #[test]
    fn ngram_edges() {
        assert_eq!(ngram_overlap("", "", 2), 1.);
        assert!(ngram_overlap("abcd", "abce", 2) > 0.);
    }
}
