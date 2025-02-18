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
                "`times` and `instantaneous_rate` must have the same length".to_string(),
            ));
        }
        if times[0] != 0.0 {
            return Err(IxaError::IxaError("`times` must start at 0.0".to_string()));
        }
        if !times.is_sorted() {
            return Err(IxaError::IxaError(
                "`times` must be sorted in ascending order".to_string(),
            ));
        }
        // We need to use the running cumulative integral to estimate the inverse rate, so calculate
        // it once and then store it for later.
        let mut cum_rates = cumulative_trapezoid_integral(&times, &instantaneous_rate)?;
        // `cum_rates` is one element shorter than `times` because its elements are the integral
        // from the start of the time series to the time at the same index in `times`. We need to
        // add the extra infectiousness that is there at the beginning of the timeseries.
        cum_rates
            .iter_mut()
            .for_each(|x| *x += instantaneous_rate[0]);
        cum_rates.insert(0, instantaneous_rate[0]);
        Ok(Self {
            times,
            instantaneous_rate,
            cum_rates,
        })
    }
    #[allow(dead_code)]
    fn get_cum_rates(&self) -> Vec<f64> {
        self.cum_rates.clone()
    }
}

impl InfectiousnessRateFn for EmpiricalRate {
    fn rate(&self, t: f64) -> f64 {
        // Find indeces of times that window `t`
        let lower_index = get_lower_index(&self.times, t);
        // Linear interpolation between those two points
        linear_interpolation(
            self.times[lower_index],
            self.times[lower_index + 1],
            self.instantaneous_rate[lower_index],
            self.instantaneous_rate[lower_index + 1],
            t,
        )
    }
    fn cum_rate(&self, t: f64) -> f64 {
        // Integrate rate function up until lower index -- over all times in the samples of the rate
        // function less than t.
        let lower_index = get_lower_index(&self.times, t);
        let mut cum_rate = self.cum_rates[lower_index];
        // Now we need to estimate the extra area from the last time in our samples of the rate
        // function to t
        // Estimate the rate at t
        let estimated_rate = linear_interpolation(
            self.times[lower_index],
            self.times[lower_index + 1],
            self.instantaneous_rate[lower_index],
            self.instantaneous_rate[lower_index + 1],
            t,
        );
        // Integrate from the last time in our samples of the rate function to t
        cum_rate += trapezoid_integral(
            &[self.times[lower_index], t],
            &[self.instantaneous_rate[lower_index], estimated_rate],
        )
        .unwrap();
        cum_rate
    }

    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        if events > *self.cum_rates.last().unwrap() {
            return None;
        }
        let lower_index = get_lower_index(&self.cum_rates, events);
        Some(linear_interpolation(
            self.cum_rates[lower_index],
            self.cum_rates[lower_index + 1],
            self.times[lower_index],
            self.times[lower_index + 1],
            events,
        ))
    }
}

/// Get the index of the largest value in `xs` that is less than or equal to `xp`
/// If there are multiple instances of that value in `xs`, a deterministically random one's index is
/// returned.
fn get_lower_index(xs: &[f64], xp: f64) -> usize {
    match xs.binary_search_by(|x| x.partial_cmp(&xp).unwrap()) {
        Ok(i) => i,
        // xp may be less than min(xs), so binary search may return Err(0)
        // We want to return 0 in this case and do extrapolation from the two smallest values of
        // `xs`, so we need to ensure that the minimum index returned is 0. This lets us have not
        // require samples of the rate function to start at time = 0.0.
        // We subtract 1 normally because binary search returns the index of the where `xp` would
        // fit, which is one after the closest value in `xs`.
        Err(i) => usize::max(i, 1) - 1,
    }
}

#[cfg(test)]
mod test {
    use ixa::IxaError;
    use statrs::assert_almost_eq;

    use super::{get_lower_index, EmpiricalRate};
    use crate::{rate_fns::InfectiousnessRateFn, utils::linear_interpolation};

    #[test]
    fn test_get_lower_index_not_included_but_inclusive() {
        let xs = vec![1.0, 2.0, 3.0];
        assert_eq!(get_lower_index(&xs, 2.5), 1);
    }

    #[test]
    fn test_get_lower_index_included() {
        let xs = vec![1.0, 2.0, 3.0];
        assert_eq!(get_lower_index(&xs, 3.0), 2);
    }

    #[test]
    fn test_get_lower_index_not_included_not_inclusive() {
        let xs = vec![1.0, 2.0, 3.0];
        assert_eq!(get_lower_index(&xs, 0.5), 0);
    }

    #[test]
    fn test_empirical_rate_times_instantaneous_rate_len_mismatch() {
        let times = vec![0.0, 1.0, 2.0];
        let instantaneous_rate = vec![0.0, 1.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`times` and `instantaneous_rate` must have the same length".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` and `instantaneous_rate` must be the same length. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_empirical_rate_times_start_at_0() {
        let times = vec![1.0, 2.0];
        let instantaneous_rate = vec![0.0, 1.0];
        let e = EmpiricalRate::new(times, instantaneous_rate).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "`times` must start at 0.0".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` must start at 0.0. Instead got {:?}",
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
                assert_eq!(msg, "`times` must be sorted in ascending order".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that `times` must be sorted. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, passed with no errors."),
        }
    }

    #[test]
    fn test_rate() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_almost_eq!(empirical.rate(1.5), 1.5, 0.0);
    }

    #[test]
    fn test_cum_rate_vector() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 0.5, 2.0]);
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
        assert_eq!(empirical.inverse_cum_rate(1.5), Some(0.5));
    }

    #[test]
    fn test_inverse_cum_rate_out_bounds() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.inverse_cum_rate(2.1), None);
    }
}
