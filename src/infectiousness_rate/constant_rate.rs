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
    fn max_rate(&self) -> f64 {
        self.rate
    }
    fn max_time(&self) -> f64 {
        self.max_time
    }
}
