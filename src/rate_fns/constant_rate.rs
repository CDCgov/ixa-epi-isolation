use ixa::IxaError;

use super::InfectiousnessRateFn;

pub struct ConstantRate {
    // A rate of infection in terms of people per unit time
    r: f64,
    // The time after which the rate of infection becomes 0
    infection_duration: f64,
}

impl ConstantRate {
    /// # Errors
    /// - The rate of infection must be non-negative.
    /// - The duration of infection must be non-negative.
    pub fn new(r: f64, infection_duration: f64) -> Result<Self, IxaError> {
        if r < 0.0 {
            return Err(IxaError::IxaError(
                "The rate of infection must be non-negative.".to_string(),
            ));
        }
        if infection_duration < 0.0 {
            return Err(IxaError::IxaError(
                "The duration of infection must be non-negative.".to_string(),
            ));
        }
        Ok(Self {
            r,
            infection_duration,
        })
    }
}

impl InfectiousnessRateFn for ConstantRate {
    fn rate(&self, t: f64) -> f64 {
        if t > self.infection_duration {
            return 0.0;
        }
        self.r
    }
    fn cum_rate(&self, t: f64) -> f64 {
        self.r * t.min(self.infection_duration)
    }
    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        let t = events / self.r;
        if t > self.infection_duration {
            None
        } else {
            Some(t)
        }
    }
    fn infection_duration(&self) -> f64 {
        self.infection_duration
    }
}

#[cfg(test)]
mod test {
    use ixa::{IxaError,assert_almost_eq};
    use super::ConstantRate;
    use super::InfectiousnessRateFn;

    #[test]
    fn test_constant_rate_errors_r_negative() {
        let e = ConstantRate::new(-1.0, 1.0).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The rate of infection must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the rate of infection must be non-negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, created a constant rate struct with no errors."),
        }
    }

    #[test]
    fn test_constant_rate_errors_infection_duration_negative() {
        let e = ConstantRate::new(1.0, -1.0).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The duration of infection must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the duration of infection must be non-negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, created a constant rate struct with no errors."),
        }
    }

    #[test]
    fn test_rate() {
        let r = ConstantRate::new(2.0, 10.0).unwrap();
        assert_almost_eq!(r.rate(5.0), 2.0, 0.0);
        assert_almost_eq!(r.rate(11.0), 0.0, 0.0);
    }

    #[test]
    fn test_cum_rate() {
        let r = ConstantRate::new(2.0, 10.0).unwrap();
        assert_almost_eq!(r.cum_rate(5.0), 10.0, 0.0);
        assert_almost_eq!(r.cum_rate(11.0), 20.0, 0.0);
    }

    #[test]
    fn test_inverse_cum_rate() {
        let r = ConstantRate::new(2.0, 10.0).unwrap();
        assert_eq!(r.inverse_cum_rate(10.0), Some(5.0));
        assert_eq!(r.inverse_cum_rate(20.0), Some(10.0));
        assert_eq!(r.inverse_cum_rate(21.0), None);
    }

    #[test]
    fn test_infection_duration() {
        let r = ConstantRate::new(2.0, 10.0).unwrap();
        assert_almost_eq!(r.infection_duration(), 10.0, 0.0);
    }
}
