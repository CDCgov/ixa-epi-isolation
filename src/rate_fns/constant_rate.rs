use super::InfectiousnessRateFn;

pub struct ConstantRate {
    rate: f64,
    max_time: f64,
}

impl ConstantRate {
    #[must_use]
    pub fn new(rate: f64, max_time: f64) -> Self {
        Self { rate, max_time }
    }
}

impl InfectiousnessRateFn for ConstantRate {
    fn rate_with_scale_offset(&self, _t: f64, scale: f64, _offset: f64) -> f64 {
        self.rate * scale
    }
    fn inverse_cum_with_scale_offset(&self, p: f64, scale: f64, offset: f64) -> Option<f64> {
        let t = p / (self.rate * scale);
        if (t + offset) > self.max_time {
            None
        } else {
            Some(t)
        }
    }
    fn max_time(&self) -> f64 {
        self.max_time
    }
}
