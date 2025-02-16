use ixa::IxaError;

use crate::utils::{linear_interpolation, midpoint_integration::midpoint_integration};

use super::InfectiousnessRateFn;

pub struct EmpiricalRate {
    // Times
    times: Vec<f64>,
    // Samples of the hazard rate at the given times
    instantaneous_rate: Vec<f64>,
}

impl EmpiricalRate {
    /// Creates a new empirical rate function from a sample of times and hazard rates
    /// # Errors
    /// - If `times` and `instantaneous_rate` do not have the same length
    /// - If `times` is not sorted in ascending order
    pub fn new(times: Vec<f64>, instantaneous_rate: Vec<f64>) -> Result<Self, IxaError> {
        if times.len() != instantaneous_rate.len() {
            return Err(IxaError::IxaError(
                "times and instantaneous_rate must have the same length".to_string(),
            ));
        }
        if !times.is_sorted() {
            return Err(IxaError::IxaError(
                "times must be sorted in ascending order".to_string(),
            ));
        }
        if times[0] != 0.0 {
            return Err(IxaError::IxaError("times must start at 0.0".to_string()));
        }
        Ok(Self {
            times,
            instantaneous_rate,
        })
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
        let mut cum_rate = midpoint_integration(
            &self.times[0..=lower_index],
            &self.instantaneous_rate[0..=lower_index],
        );
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
        cum_rate += midpoint_integration(
            &[self.times[lower_index], t],
            &[self.instantaneous_rate[lower_index], estimated_rate],
        );
        cum_rate
    }

    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        let cum_rates = self
            .times
            .windows(2)
            .zip(self.instantaneous_rate.windows(2))
            .scan(0.0, |state, (ts, rates)| {
                *state += midpoint_integration(ts, rates);
                Some(*state)
            })
            .collect::<Vec<f64>>();
        if events > *cum_rates.last().unwrap() {
            return None;
        }
        let lower_index = get_lower_index(&cum_rates, events);
        Some(linear_interpolation(
            cum_rates[lower_index],
            cum_rates[lower_index + 1],
            self.times[lower_index],
            self.times[lower_index + 1],
            events,
        ))
    }
}

fn get_lower_index(xs: &[f64], xp: f64) -> usize {
    match xs.binary_search_by(|x| x.partial_cmp(&xp).unwrap()) {
        Ok(i) => i,
        Err(i) => i - 1,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    #[test]
    fn test_rate() {}

    #[test]
    fn test_cum_rate() {}

    #[test]
    fn test_inverse_cum_rate() {}
}
