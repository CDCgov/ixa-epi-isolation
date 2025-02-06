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
    fn get_rate(&self, _t: f64) -> f64 {
        self.rate
    }
    fn scale_rate(&self, _t: f64, _offset: f64, factor: f64) -> f64 {
        self.rate * factor
    }
    fn max_rate(&self) -> f64 {
        self.rate
    }
    fn max_time(&self) -> f64 {
        self.max_time
    }
    fn inverse_cum(&self, p: f64) -> Option<f64> {
        self.scale_inverse_cum(p, 0.0, 1.0)
    }
    fn scale_inverse_cum(&self, p: f64, offset: f64, factor: f64) -> Option<f64> {
        let t = p / (self.rate * factor);
        if (t + offset) > self.max_time {
            None
        } else {
            Some(t)
        }
    }
}
