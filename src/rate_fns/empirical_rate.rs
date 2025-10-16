use ixa::IxaError;

use crate::utils::{cumulative_trapezoid_integral, linear_interpolation, trapezoid_integral};

use super::InfectiousnessRateFn;

pub struct EmpiricalRate {
    // Times at which we have samples of the infectiousness rate
    times: Vec<f64>,
    // Samples of the instantaneous infectiousness rate at the corresponding times
    instantaneous_rate: Vec<f64>,
    // Estimated cumulative infectiousness elapsed at a given time
    cum_rates: Vec<f64>,
}

impl EmpiricalRate {
    /// Creates a new empirical rate function from a sample of times and infectiousness rates
    /// # Errors
    /// - If `times` and `instantaneous_rate` do not have the same length and are less than two
    ///   elements long.
    /// - If `times` is not sorted in ascending order.
    /// - If `times` or `instantaneous_rate` contain negative values.
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
        if !times.is_sorted() {
            return Err(IxaError::IxaError(
                "`times` must be sorted in ascending order.".to_string(),
            ));
        }
        // Because times are sorted, we know that if the first value is greater than zero,
        // everything must be greater than 0.
        if times[0] < 0.0 {
            return Err(IxaError::IxaError(
                "`times` must be non-negative.".to_string(),
            ));
        }
        if instantaneous_rate.iter().any(|&x| x < 0.0) {
            return Err(IxaError::IxaError(
                "`instantaneous_rate` must be non-negative.".to_string(),
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

        Ok(Self {
            cum_rates,
            ..empirical_rate_no_cum
        })
    }

    #[allow(dead_code)]
    /// Used exclusively in tests for checking that we have created the cumulative rates correctly.
    fn get_cum_rates(&self) -> Vec<f64> {
        self.cum_rates.clone()
    }

    /// Private function that returns all the values used in the rate function interpolation
    /// process. In particular, returns a tuple where the first two elements are the result of
    /// `get_lower_index(times, t)` (i.e., the first element is the index in `times` of the greatest
    /// value less than `t` and the second element is the minimum of that index and the length of
    /// `times` - 2) and the third element is the estimated rate at time `t`.
    fn lower_index_and_rate(&self, t: f64) -> (usize, usize, f64) {
        // Get index of greatest value in `times` less than or equal to `t` and an adjusted index
        // that ensures there is a value to the right of the index in `times` for interpolation.
        let (integration_index, interpolation_index) = get_lower_index(&self.times, t);
        // Return both indices and the estimated rate at time `t`.
        (
            integration_index,
            interpolation_index,
            // Linear interpolation between the two points that window `t`.
            // Ensure the returned rate is not negative even if extrapolation tells us it should be.
            f64::max(
                0.0,
                linear_interpolation(
                    self.times[interpolation_index],
                    self.times[interpolation_index + 1],
                    self.instantaneous_rate[interpolation_index],
                    self.instantaneous_rate[interpolation_index + 1],
                    t,
                ),
            ),
        )
    }
}

/// Returns a pair of indices referring to locations in `xs`. The first is the index of the largest
/// value in `xs` that is less than or equal to `xp` (unless `xp` < `min(xs)` in which case it
/// returns 0). This index is used for querying that value or a vector calculated from the
/// corresponding values in `xs`. The second index is an adjusted version of the first index that
/// ensures that there is at least one value to the right of the index in `xs`. In other words, if
/// the first index is the last index in `xs`, the second index will be the second to last index in
/// `xs`.
/// If there are multiple values in `xs` that satisfy being the largest value less than or equal to
/// `xp`, the function checks to return the smallest of those values.
/// Assumes that `xs` is sorted in ascending order. However, this is a private function only called
/// within `EmpiricalRate` where the values are already checked to be sorted.
fn get_lower_index(xs: &[f64], xp: f64) -> (usize, usize) {
    let mut integration_index = match xs.binary_search_by(|x| x.partial_cmp(&xp).unwrap()) {
        Ok(i) => i,
        // xp may be less than min(xs), so binary search may return Err(0). This case can arise
        // because we do not require that the samples of the rate function start at time = 0.0 or if
        // the `events` called in `inverse_cum_rate` is less than the first value in `cum_rates`.
        // We still want to be able to query a value of `cum_rates`, so we need to return 0.
        // We subtract 1 normally because binary search returns the index of the where `xp` would
        // go if it were inserted, which is one after the greatest value less than `xp` in `xs`.
        Err(i) => usize::max(i, 1) - 1,
    };

    // We want to make sure we return the smallest index in the case where there are multiple values
    // in `xs` that are equal to the value at `integration_index`.
    // To do this, we "walk left" along the array until we hit a value not equal to the value in
    // question.
    let val = xs[integration_index];
    while integration_index > 0 {
        #[allow(clippy::float_cmp)]
        if xs[integration_index - 1] != val {
            break;
        }
        integration_index -= 1;
    }

    // We need to return the integration index and an adjusted version of that index for
    // interpolation.
    (
        integration_index,
        // In the case where xp >= max(xs), we want to return the second to last index so that we
        // have two points over which to do interpolation.
        usize::min(integration_index, xs.len() - 2),
    )
}

impl InfectiousnessRateFn for EmpiricalRate {
    fn rate(&self, t: f64) -> f64 {
        if t < self.times[0] {
            return 0.0;
        }
        if t > self.times[self.times.len() - 1] {
            return 0.0;
        }
        self.lower_index_and_rate(t).2
    }

    fn cum_rate(&self, t: f64) -> f64 {
        if t < self.times[0] {
            return 0.0;
        }
        // We use a two-step process: first, we use the pre-calculated cumulative rates vector to
        // get the cumulative rate up until the greatest value in `times` less than or equal to `t`.
        // Then, we estimate the extra cumulative rate from the last time in our samples of the rate
        // function to `t`.

        // We need the get the index of the greatest value in `times` less than or equal to `t` for
        // the first step (querying the pre-calculated cumulative rates vector value at that index).
        // Later, we will need the estimated rate at `t` for the second step, so we get both here.
        let (integration_index, _interpolation_index, estimated_rate) =
            self.lower_index_and_rate(t);
        let mut cum_rate = self.cum_rates[integration_index];
        if t > self.times[self.times.len() - 1] {
            // rates for times greater than the last time are 0, so cum_rate stays the same
            return cum_rate;
        }

        // Now we need to estimate the extra area from the last time in our samples of the rate
        // function to t.
        cum_rate += trapezoid_integral(
            &[self.times[integration_index], t],
            &[self.instantaneous_rate[integration_index], estimated_rate],
        )
        .unwrap();
        cum_rate
    }

    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        // If events is greater than the maximum value in `cum_rates`, we return None because this
        // rate function cannot produce that much infectiousness.
        if events > self.cum_rates[self.cum_rates.len() - 1] {
            return None;
        }
        // We want to return the time at which `events` would have happened. At a high level, this
        // consists of a two-step process -- we find the minimum time at which the greatest value
        // in `cum_rates` less than or equal to `events` would have occurred. Then, we estimate the
        // extra time that has passed since that number of events, accounting for corner cases of
        // the rate potentially being zero over a window.
        // We know that `cum_rates` is the running total of how many events have happened by a given
        // time, so we start be finding the index of the greatest value in `cum_rates` less than or
        // equal to `events` and using that to figure out at least how much time has passed.
        let (mut integration_index, _interpolation_index) =
            get_lower_index(&self.cum_rates, events);

        // We need the number of events beyond the last value in `cum_rates` to estimate the extra
        // time that has passed. We describe a formula below for relating this value to the time.
        let extra_events = events - self.cum_rates[integration_index];

        // There's an odd corner case for when the rate is zero over a time window -- in this case,
        // the cumulative rate is constant over that period. We've obtained the minimum time at
        // which the number of events in question can occur, so we don't need to do any further
        // calculations.
        if extra_events == 0.0 {
            return Some(self.times[integration_index]);
        }

        // In the case where extra_events > 0, we need to make sure that we're basing our minimum
        // time on the *maximum* occurrence of the value at `cum_rates[integration_index]`.
        // Basically, if the rate is 0 over some time period, there can't be any events occurring in
        // that window, so we have to instead use the first time at which the rate is again
        // positive. To obtain this value, we walk right until we find a non-zero rate.
        // We know that `events` is now less than `max(cum_rates)` because (a) we already returned
        // None if it was greater and (b) we just handled the case above where `events` is the max
        // value because then `extra_events` would be 0. Therefore, we know that there is an
        // increase in cum_rates that happens at some value to the right, so this walk right will
        // always succeed.
        while integration_index < self.cum_rates.len() - 1 {
            #[allow(clippy::float_cmp)]
            if self.cum_rates[integration_index + 1] != self.cum_rates[integration_index] {
                break;
            }
            integration_index += 1;
        }

        // Now, we are ready to estimate the time at which `events` would have occurred.
        // We know that events are the integral of rate over time, so we need to solve the following
        // for t:
        // extra_events = integral_{time_passed}^{time_passed+t}, rate(\tau) d\tau)
        // We assume that rates change linearly over adjacent time windows.
        // extra_events = integral_{0}^{t}, (rate(time_passed) + slope*\tau) d\tau)
        // extra_events = rate(time_passed) * t + t^2 * slope / 2 = (rate(time_passed) + t * slope / 2) * t
        // 0 = t^2 * slope / 2 + rate(time_passed) * t - extra_events
        let delta_y = self.instantaneous_rate[integration_index + 1]
            - self.instantaneous_rate[integration_index];
        let delta_x = self.times[integration_index + 1] - self.times[integration_index];
        let slope = delta_y / delta_x;

        // Because extra_events is > 0 by this point, we know that this quadratic equation will
        // always have at least one root. We can manually calculate the root.
        // But first, we have to check that we really have a quadratic equation and not a linear
        // equation.
        if slope == 0.0 {
            return Some(
                self.times[integration_index]
                    + extra_events / self.instantaneous_rate[integration_index],
            );
        }
        let discriminant =
            f64::powi(self.instantaneous_rate[integration_index], 2) + 2.0 * slope * extra_events;
        // Finally, the reason this quadratic solution arises is because we are inverting a
        // quadratic function. Therefore, we can show that we always want the solution where we add
        // the square root of the discriminant because when we make the quadratic function, we are
        // *accumulating* extra area.
        let t = (-self.instantaneous_rate[integration_index] + f64::sqrt(discriminant)) / slope;
        Some(self.times[integration_index] + t)
    }

    fn infection_duration(&self) -> f64 {
        self.times[self.times.len() - 1]
    }
}

#[cfg(test)]
mod test {
    use ixa::IxaError;
    use ixa::assert_almost_eq;

    use super::{get_lower_index, EmpiricalRate};
    use crate::rate_fns::InfectiousnessRateFn;

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
        let empirical = EmpiricalRate::new(vec![1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0]).unwrap();
        assert_eq!(empirical.lower_index_and_rate(0.5), (0, 0, 0.5));
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
        // The cumulative rates should start at 0.0 because we assume that infectiousness is 0.0
        // before the first time in the time series.
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_cum_rate_eval() {
        let empirical =
            EmpiricalRate::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![0.0, 1.0, 2.0, 1.0, 0.0])
                .unwrap();
        assert_almost_eq!(empirical.cum_rate(1.5), 1.125, 0.0);
        assert_almost_eq!(empirical.cum_rate(2.5), 2.875, 0.0);
    }

    #[test]
    fn test_inverse_cum_rate_in_bounds() {
        let empirical =
            EmpiricalRate::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![0.0, 1.0, 2.0, 1.0, 0.0])
                .unwrap();
        // Inverse of example values above.
        // The rate is increasing, so slope is positive, so this is the positive quadratic root.
        assert_eq!(empirical.inverse_cum_rate(1.125), Some(1.5));
        // Slope is decreasing; negative quadratic root.
        assert_eq!(empirical.inverse_cum_rate(2.875), Some(2.5));
    }

    #[test]
    fn test_inverse_cum_rate_out_bounds_above() {
        let empirical = EmpiricalRate::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]).unwrap();
        assert_eq!(empirical.inverse_cum_rate(2.1), None);
    }

    #[test]
    fn test_inverse_cum_rate_out_bounds_below() {
        // Want to test that if we need to calculate the inverse cum rate for a number of events
        // less than the minimum value in the cumulative rates vector, do we actually return a
        // value less than that minimum value? In other words, does our pre-calculation of the
        // cumulative rates vector + extra integration work for the case when that extra integration
        // needs to return a negative value?
        let empirical = EmpiricalRate::new(vec![1.0, 2.0, 3.0], vec![1.0, 1.0, 1.0]).unwrap();
        assert_eq!(empirical.inverse_cum_rate(0.5), Some(1.5));
    }

    #[test]
    fn test_cum_rate_below_zero_rate() {
        let empirical = EmpiricalRate::new(vec![1.0, 2.0, 3.0], vec![1.0, 3.0, 5.0]).unwrap();
        // At t = 0, the rate would be -1.0, but we should return 0.0.
        assert_almost_eq!(empirical.cum_rate(0.0), 0.0, 0.0);
        // We assume that infectiousness is 0.0 before the first time in the time series, so the
        // cumulative rate should be 0.0 (the first value in the vector).
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 2.0, 6.0]);
    }

    #[test]
    fn test_inverse_cum_rate_plateaus() {
        let empirical =
            EmpiricalRate::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![1.0, 1.0, 0.0, 0.0, 1.0])
                .unwrap();
        // The cumulative rates should plateau.
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 1.0, 1.5, 1.5, 2.0]);
        // Searching for the index for `events = 1.6` should return 2.
        // (Without our code to check for this, can confirm that in this particular example we
        // return (3, 3).)
        assert_eq!(get_lower_index(&empirical.get_cum_rates(), 1.6), (2, 2));
        // Getting the time of an inverse cumulative rate of 1.5 should return 2.0 (first index).
        assert_eq!(empirical.inverse_cum_rate(1.5), Some(2.0));
        // And getting the inverse cumulative rate for a slightly greater value should return a time
        // greater than 3.0 because the rate starts moving again after t = 3.0.
        assert!(empirical.inverse_cum_rate(1.6).unwrap() > 3.0);
    }

    #[test]
    fn test_infection_duration() {
        let empirical = EmpiricalRate::new(
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            vec![1.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        )
        .unwrap();
        assert_almost_eq!(empirical.infection_duration(), 5.0, 0.0);

        // Show that the infection duration is agnostic to the value in `instantaneous_rate`
        let empirical = EmpiricalRate::new(
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            vec![1.0, 1.0, 0.0, 0.0, 1.0, 1.0],
        )
        .unwrap();
        assert_almost_eq!(empirical.infection_duration(), 5.0, 0.0);
    }

    #[test]
    fn test_rates_below_zero_rate() {
        let empirical = EmpiricalRate::new(vec![1.0, 2.0], vec![1.0, 1.0]).unwrap();
        // Rate below first value in time series should be 0
        assert_almost_eq!(empirical.rate(0.5), 0.0, 0.0);
        // Cum rate below first value in time series should be 0
        assert_almost_eq!(empirical.cum_rate(0.5), 0.0, 0.0);
        // Cum rate should always start at 0
        assert_eq!(empirical.get_cum_rates(), vec![0.0, 1.0]);
        // Rate above last value in time series should be 0
        assert_almost_eq!(empirical.rate(2.1), 0.0, 0.0);
        // Cum rate should not increase beyond cum rate of last value in time series
        assert_almost_eq!(empirical.cum_rate(2.1), 1.0, 0.0);
        // Inverse cum rate should always return a time that is greater than the minimum time
        assert!(empirical.inverse_cum_rate(f64::EPSILON).unwrap() > 1.0);
    }
}
