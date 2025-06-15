#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    Passing,
    Failing,
    Flaky,
}
pub fn classify(passes: usize, total: usize, min: f64, max: f64) -> Stability {
    if total == 0 {
        return Stability::Failing;
    }
    let rate = passes as f64 / total as f64;
    if rate <= min {
        Stability::Failing
    } else if rate >= max {
        Stability::Passing
    } else {
        Stability::Flaky
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classes() {
        assert_eq!(classify(0, 3, 0.01, 0.99), Stability::Failing);
        assert_eq!(classify(3, 3, 0.01, 0.99), Stability::Passing);
        assert_eq!(classify(2, 3, 0.01, 0.99), Stability::Flaky);
    }
    #[test]
    fn zero() {
        assert_eq!(classify(0, 0, 0.1, 0.9), Stability::Failing);
    }
}
