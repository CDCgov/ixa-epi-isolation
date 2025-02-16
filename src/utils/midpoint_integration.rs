#[must_use]
pub fn midpoint_integration(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| (x[1] - x[0]) * (y[0] + y[1]) / 2.0)
        .sum()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_midpoint_integration() {}
}
