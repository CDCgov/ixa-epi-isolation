#[must_use]
pub fn midpoint_integration(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| (x[1] - x[0]) * (y[0] + y[1]) / 2.0)
        .sum()
}

#[must_use]
pub fn cumulative_integral(x: &[f64], y: &[f64]) -> Vec<f64> {
    x.windows(2)
        .zip(y.windows(2))
        .scan(0.0, |state, (ts, rates)| {
            *state += midpoint_integration(ts, rates);
            Some(*state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use statrs::assert_almost_eq;

    use super::{cumulative_integral, midpoint_integration};

    #[test]
    fn test_midpoint_integration_simple() {
        let x = vec![0.0, 1.0];
        let y = vec![0.0, 1.0];
        assert_almost_eq!(midpoint_integration(&x, &y), 0.5, 0.0);
    }

    #[test]
    fn test_midpoint_integration_vector() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y: Vec<f64> = x.iter().map(|x| x * x).collect();
        assert_almost_eq!(midpoint_integration(&x, &y), 21.5, 0.0);
    }

    #[test]
    fn test_cumulative_integral() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y: Vec<f64> = x.iter().map(|x| x * x).collect();
        let cum_y = cumulative_integral(&x, &y);
        assert_eq!(cum_y, vec![2.5, 9.0, 21.5]);
    }
}
