use super::InfectiousnessRateFn;

pub struct ConstantRate {
    // A rate of infection in terms of people per unit time
    r: f64,
    // The time after which the rate of infection becomes 0
    infection_duration: f64,
}

impl ConstantRate {
    #[must_use]
    pub fn new(r: f64, infection_duration: f64) -> Self {
        Self {
            r,
            infection_duration,
        }
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
    fn infection_duration_remaining(&self, t: f64) -> f64 {
        self.infection_duration - t
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use super::ConstantRate;
    use super::InfectiousnessRateFn;

    #[test]
    fn test_rate() {
        let r = ConstantRate::new(2.0, 10.0);
        assert_eq!(r.rate(5.0), 2.0);
        assert_eq!(r.rate(11.0), 0.0);
    }

    #[test]
    fn test_cum_rate() {
        let r = ConstantRate::new(2.0, 10.0);
        assert_eq!(r.cum_rate(5.0), 10.0);
        assert_eq!(r.cum_rate(11.0), 20.0);
    }

    #[test]
    fn test_inverse_cum_rate() {
        let r = ConstantRate::new(2.0, 10.0);
        assert_eq!(r.inverse_cum_rate(10.0), Some(5.0));
        assert_eq!(r.inverse_cum_rate(20.0), Some(10.0));
        assert_eq!(r.inverse_cum_rate(21.0), None);
    }
}
