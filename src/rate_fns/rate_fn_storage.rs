use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt};
use serde::Serialize;

use super::rate_fn::InfectiousnessRateFn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RateFnId(usize);

struct RateFnContainer {
    rates: Vec<Box<dyn InfectiousnessRateFn>>,
}

define_data_plugin!(
    RateFnPlugin,
    RateFnContainer,
    RateFnContainer { rates: Vec::new() }
);

define_rng!(InfectiousnessRateRng);

pub trait InfectiousnessRateExt {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>) -> RateFnId;
    fn get_random_rate_function(&mut self) -> RateFnId;
    fn get_rate_fn(&self, index: RateFnId) -> &dyn InfectiousnessRateFn;
}

impl InfectiousnessRateExt for Context {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>) -> RateFnId {
        let container = self.get_data_container_mut(RateFnPlugin);
        container.rates.push(dist);
        RateFnId(container.rates.len() - 1)
    }

    fn get_random_rate_function(&mut self) -> RateFnId {
        let max = self.get_data_container_mut(RateFnPlugin).rates.len();
        RateFnId(self.sample_range(InfectiousnessRateRng, 0..max))
    }

    fn get_rate_fn(&self, index: RateFnId) -> &dyn InfectiousnessRateFn {
        self.get_data_container(RateFnPlugin)
            .expect("Expected rate function to exist")
            .rates[index.0]
            .as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use ixa::Context;

    struct TestRateFn;

    impl InfectiousnessRateFn for TestRateFn {
        fn rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn cum_rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn inverse_cum_rate(&self, _events: f64) -> Option<f64> {
            Some(1.0)
        }
    }

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_random() {
        let mut context = init_context();

        let rate_fn = TestRateFn {};
        context.add_rate_fn(Box::new(rate_fn));

        let i = context.get_random_rate_function();
        assert!(i.0 == 0);
        assert_eq!(context.get_rate_fn(i).rate(0.0), 1.0);
    }
}
