use ixa::IxaError;

/// Calculate the integral of `y` by taking the estimated midpoint of each interval (trapezoidal
/// integration).
#[must_use]
pub fn midpoint_integration(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| (x[1] - x[0]) * (y[0] + y[1]) / 2.0)
        .sum()
}

/// Calculate the definite integral of a function `y` at each value in `x`.
/// Returns a vector that is one element shorter than `y`.
/// # Errors
/// - If `x` and `y` do not have the same length
pub fn cumulative_integral(x: &[f64], y: &[f64]) -> Result<Vec<f64>, IxaError> {
    if x.len() != y.len() {
        return Err(IxaError::IxaError(
            "`x` and `y` must have the same length.".to_string(),
        ));
    }
    Ok(x.windows(2)
        .zip(y.windows(2))
        .scan(0.0, |state, (ts, rates)| {
            *state += midpoint_integration(ts, rates);
            Some(*state)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use ixa::IxaError;
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
        let cum_y = cumulative_integral(&x, &y).unwrap();
        assert_eq!(cum_y, vec![2.5, 9.0, 21.5]);
    }

    #[test]
    fn test_cumulative_integral_x_y_len() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0];

        let e = cumulative_integral(&x, &y).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`x` and `y` must have the same length.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `x` and `y` must be the same length. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }
}
