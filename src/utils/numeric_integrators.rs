use ixa::IxaError;

/// Calculate the integral of `y` as a function of `x` using the trapezoid rule -- estimating that
/// the value of `y` between two adjacent `x` values is the midpoint of the start and end `y` value/
/// `y` changes linearly between each adjacent set of `x` values.
/// # Errors
/// - If `x` and `y` do not have the same length.
pub fn trapezoid_integral(x: &[f64], y: &[f64]) -> Result<f64, IxaError> {
    if x.len() != y.len() {
        return Err(IxaError::IxaError(
            "`x` and `y` must have the same length.".to_string(),
        ));
    }
    Ok(x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| (x[1] - x[0]) * (y[0] + y[1]) / 2.0)
        .sum())
}

/// Calculate the definite integral of `y` as a function of `x` from `x[0]` to each value in `x`
/// using `trapezoid_integral`. Returns a vector that is one element shorter than `y`, so the first
/// element is the integral from `x[0]` to `x[1]`, the second element is the integral from `x[0]`
/// to `x[2]`, and so on.
/// # Errors
/// - If `x` and `y` do not have the same length.
/// - If `x` is not sorted in ascending order.
#[allow(clippy::missing_panics_doc)]
pub fn cumulative_trapezoid_integral(x: &[f64], y: &[f64]) -> Result<Vec<f64>, IxaError> {
    if x.len() != y.len() {
        return Err(IxaError::IxaError(
            "`x` and `y` must have the same length.".to_string(),
        ));
    }
    if !x.is_sorted() {
        return Err(IxaError::IxaError(
            "`x` must be sorted in ascending order.".to_string(),
        ));
    }
    Ok(x.windows(2)
        .zip(y.windows(2))
        .scan(0.0, |state, (ts, rates)| {
            *state += trapezoid_integral(ts, rates).unwrap();
            Some(*state)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use ixa::IxaError;
    use statrs::assert_almost_eq;

    use super::{cumulative_trapezoid_integral, trapezoid_integral};

    #[test]
    fn test_trapezoid_integration_simple() {
        let x = vec![0.0, 1.0];
        let y = vec![0.0, 1.0];
        assert_almost_eq!(trapezoid_integral(&x, &y).unwrap(), 0.5, 0.0);
    }

    #[test]
    fn test_trapezoid_integration_vector() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y: Vec<f64> = x.iter().map(|x| x * x).collect();
        assert_almost_eq!(trapezoid_integral(&x, &y).unwrap(), 21.5, 0.0);
    }

    #[test]
    fn test_trapezoid_integral_x_y_len() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0];

        let e = trapezoid_integral(&x, &y).err();
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

    #[test]
    fn test_cumulative_integral() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y: Vec<f64> = x.iter().map(|x| x * x).collect();
        let cum_y = cumulative_trapezoid_integral(&x, &y).unwrap();
        assert_eq!(cum_y, vec![2.5, 9.0, 21.5]);
    }

    #[test]
    fn test_cumulative_integral_x_y_len() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0];

        let e = cumulative_trapezoid_integral(&x, &y).err();
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

    #[test]
    fn test_cumulative_integral_x_sorted() {
        let x = vec![2.0, 1.0, 3.0, 4.0];
        let y = vec![1.0, 2.0, 3.0, 4.0];

        let e = cumulative_trapezoid_integral(&x, &y).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`x` must be sorted in ascending order.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `x` must be sorted in ascending order. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }
}
