use ixa::IxaError;

use crate::utils::{cumulative_trapezoid_integral, linear_interpolation, trapezoid_integral};

use super::InfectiousnessRateFn;

pub struct EmpiricalRate {
    // Times
    times: Vec<f64>,
    // Samples of the hazard rate at the given times
    instantaneous_rate: Vec<f64>,
    cum_rates: Vec<f64>,
}

impl EmpiricalRate {
    /// Creates a new empirical rate function from a sample of times and hazard rates
    /// # Errors
    /// - If `times` and `instantaneous_rate` do not have the same length
    /// - If `times` is not sorted in ascending order
    pub fn new(times: Vec<f64>, instantaneous_rate: Vec<f64>) -> Result<Self, IxaError> {
        if times.len() != instantaneous_rate.len() {
            return Err(IxaError::IxaError(
                "`times` and `instantaneous_rate` must have the same length.".to_string(),
            ));
        }
        if times.len() <= 1 {
            return Err(IxaError::IxaError(
                "`times` and `instantaneous_rate` must have at least two elements.".to_string(),
            ));
        }
        if times.iter().any(|&x| x < 0.0) {
            return Err(IxaError::IxaError(
                "`times` must be non-negative.".to_string(),
            ));
        }
        if instantaneous_rate.iter().any(|&x| x < 0.0) {
            return Err(IxaError::IxaError(
                "`instantaneous_rate` must be non-negative.".to_string(),
            ));
        }
        if !times.is_sorted() {
            return Err(IxaError::IxaError(
                "`times` must be sorted in ascending order.".to_string(),
            ));
        }
        let empirical_rate_no_cum = Self {
            times,
            instantaneous_rate,
            cum_rates: vec![],
        };
        // We need to use the running cumulative integral to estimate the inverse rate, so calculate
        // it once and then store it for later.
        let mut cum_rates = cumulative_trapezoid_integral(
            &empirical_rate_no_cum.times,
            &empirical_rate_no_cum.instantaneous_rate,
        )?;
        // `cum_rates` is one element shorter than `times` because its elements are the integral
        // from the start of the time series to the time at the same index in `times` (omitting the
        // integral from the first value in the time series to itself).
        // We add that zero here:
        cum_rates.insert(0, 0.0);
        // Next we account for there being infectiousness potentially before the first value in the
        // timeseries -- i.e., if the rate timeseries does not start at 0.
        // We need to calculate that infectiousness and add it to our cumulative rates.
        let (_, _, estimated_rate_zero) = empirical_rate_no_cum.lower_index_and_rate(0.0);
        let pre_times_zero_infectiousness = trapezoid_integral(
            &[0.0, empirical_rate_no_cum.times[0]],
            &[
                estimated_rate_zero,
                empirical_rate_no_cum.instantaneous_rate[0],
            ],
        )?;
        cum_rates
            .iter_mut()
            .for_each(|x| *x += pre_times_zero_infectiousness);
        Ok(Self {
            cum_rates,
            ..empirical_rate_no_cum
        })
    }
    #[allow(dead_code)]
    /// Helper function to get out the cumulative rates at each time.
    fn get_cum_rates(&self) -> Vec<f64> {
        self.cum_rates.clone()
    }
    /// Helper function to return all the elements we may need from the rate function interpolation
    /// routine.
    fn lower_index_and_rate(&self, t: f64) -> (usize, usize, f64) {
        // Find indeces of times that window `t`
        let (integration_index, interpolation_index) = get_lower_index(&self.times, t);
        // Linear interpolation between those two points
        (
            integration_index,
            interpolation_index,
            linear_interpolation(
                self.times[interpolation_index],
                self.times[interpolation_index + 1],
                self.instantaneous_rate[interpolation_index],
                self.instantaneous_rate[interpolation_index + 1],
                t,
            ),
        )
    }
}

impl InfectiousnessRateFn for EmpiricalRate {
    fn rate(&self, t: f64) -> f64 {
        // Ensure the rate cannot be negative
        f64::max(0.0, self.lower_index_and_rate(t).2)
    }
    fn cum_rate(&self, t: f64) -> f64 {
        // Integrate rate function up until lower index -- over all times in the samples of the rate
        // function less than t.
        let (integration_index, _, estimated_rate) = self.lower_index_and_rate(t);
        let mut cum_rate = self.cum_rates[integration_index];
        // Now we need to estimate the extra area from the last time in our samples of the rate
        // function to t
        // Integrate from the last time in our samples of the rate function to t
        cum_rate += trapezoid_integral(
            &[self.times[integration_index], t],
            &[self.instantaneous_rate[integration_index], estimated_rate],
        )
        .unwrap();
        cum_rate
    }

    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        if events > *self.cum_rates.last().unwrap() {
            return None;
        }
        let (_, interpolation_index) = get_lower_index(&self.cum_rates, events);
        Some(linear_interpolation(
            self.cum_rates[interpolation_index],
            self.cum_rates[interpolation_index + 1],
            self.times[interpolation_index],
            self.times[interpolation_index + 1],
            events,
        ))
    }
}

/// Get the index of the largest value in `xs` that is less than or equal to `xp`, and the index of
/// the largest value in `xs` that is less than or equal to `xp` but accounts for there being at
/// least one value in `xs` that is greater.
/// If there are multiple instances of the greatest value less than or equal to `xp` in `xs`, a
/// deterministically random one's index is returned.
fn get_lower_index(xs: &[f64], xp: f64) -> (usize, usize) {
    let integration_index = match xs.binary_search_by(|x| x.partial_cmp(&xp).unwrap()) {
        Ok(i) => i,
        // xp may be less than min(xs), so binary search may return Err(0)
        // We want to return 0 in this case and do extrapolation from the two smallest values of
        // `xs`, so we need to ensure that the minimum index returned is 0. This lets us have not
        // require samples of the rate function to start at time = 0.0.
        // We subtract 1 normally because binary search returns the index of the where `xp` would
        // fit, which is one after the closest value in `xs`.
        Err(i) => usize::max(i, 1) - 1,
    };

    // In the case where xp >= max(xs), we want to return the second to last index so that we
    // have two points over which to do interpolation.
    (
        integration_index,
        usize::min(integration_index, xs.len() - 2),
    )
}

#[cfg(test)]
mod test {
    use ixa::IxaError;
    use statrs::assert_almost_eq;

    use super::{get_lower_index, EmpiricalRate};
    use crate::{rate_fns::InfectiousnessRateFn, utils::linear_interpolation};

    #[test]
    fn test_get_lower_index_not_included_but_inclusive() {
        let xs = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(get_lower_index(&xs, 3.5), (2, 2));
        assert_eq!(get_lower_index(&xs, 2.5), (1, 1));
        assert_eq!(get_lower_index(&xs, 1.5), (0, 0));
    }

    #[test]
    fn test_get_lower_index_included() {
        let xs = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(get_lower_index(&xs, 4.0), (3, 2));
        assert_eq!(get_lower_index(&xs, 3.0), (2, 2));
        assert_eq!(get_lower_index(&xs, 2.0), (1, 1));
        assert_eq!(get_lower_index(&xs, 1.0), (0, 0));
    }

    #[test]
    fn test_get_lower_index_not_included_not_inclusive_below() {
        let xs = vec![1.0, 2.0];
        assert_eq!(get_lower_index(&xs, 0.5), (0, 0));
    }

    #[test]
    fn test_get_lower_index_not_included_not_inclusive_above() {
        let xs = vec![1.0, 2.0, 3.0];
        assert_eq!(get_lower_index(&xs, 3.5), (2, 1));
    }

    #[test]
    fn test_empirical_rate_times_instantaneous_rate_len_mismatch() {
        let times = vec![0.0, 1.0, 2.0];
        let instantaneous_rate = vec![0.0, 1.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`times` and `instantaneous_rate` must have the same length.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` and `instantaneous_rate` must be the same length. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_empirical_rate_times_non_negative() {
        let times = vec![-1.0, 0.0, 1.0];
        let instantaneous_rate = vec![0.0, 1.0, 2.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`times` must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` must be non-negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_empirical_rate_times_at_least_len_two() {
        let times = vec![0.0];
        let instantaneous_rate = vec![0.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`times` and `instantaneous_rate` must have at least two elements.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` and `instantaneous_rate` must have at least two elements. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_empirical_rate_instantaneous_rate_non_negative() {
        let times = vec![0.0, 1.0];
        let instantaneous_rate = vec![0.0, -0.2];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`instantaneous_rate` must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `instantaneous_rate` must be non-negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_empirical_rate_times_not_sorted() {
        let times = vec![0.0, 0.0, 3.0, 2.0];
        let instantaneous_rate = vec![0.0, 1.0, 2.0, 3.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "`times` must be sorted in ascending order.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that `times` must be sorted. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_internal_index_rate_t_within_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.lower_index_and_rate(1.5), (1, 1, 1.5));
    }

    #[test]
    fn test_internal_index_rate_t_provided() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.lower_index_and_rate(1.0), (1, 1, 1.0));
    }

    #[test]
    fn test_internal_index_rate_t_above_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.lower_index_and_rate(2.5), (2, 1, 2.5));
    }

    #[test]
    fn test_internal_index_rate_t_below_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.lower_index_and_rate(-0.5), (0, 0, -0.5));
    }

    #[test]
    fn test_rate() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_almost_eq!(empirical.rate(1.5), 1.5, 0.0);
    }

    #[test]
    fn test_rate_not_negative() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_almost_eq!(empirical.rate(-0.5), 0.0, 0.0);
    }

    #[test]
    fn test_cum_rate_vector() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 0.5, 2.0]);
    }

    #[test]
    fn test_cum_rate_vector_nonzero_start() {
        let empirical = EmpiricalRate::new(vec![1.0, 2.0, 3.0], vec![1.0, 1.0, 1.0]).unwrap();
        assert_eq!(empirical.get_cum_rates(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_cum_rate_eval() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_almost_eq!(empirical.cum_rate(1.5), 1.125, 0.0);
    }

    #[test]
    fn test_inverse_cum_rate_in_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(
            empirical.inverse_cum_rate(1.125),
            // The rate function is linear, so it's integral (cum rate) is quadratic, and not
            // linearly invertible. This is why we don't get exactly 1.5 here.
            // We show exact linear invertibility in the next test.
            Some(linear_interpolation(0.5, 2.0, 1.0, 2.0, 1.125))
        );
    }

    #[test]
    fn test_inverse_cum_rate_in_bounds_invertible() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![1.0, 1.0, 1.0]).unwrap();
        assert_eq!(empirical.inverse_cum_rate(1.5), Some(1.5));
    }

    #[test]
    fn test_inverse_cum_rate_out_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.inverse_cum_rate(2.1), None);
    }
}
