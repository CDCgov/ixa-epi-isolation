use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, Context, ContextPeopleExt,
    ContextRandomExt, PersonId,
};

pub mod constant_rate;
pub use constant_rate::ConstantRate;

define_person_property_with_default!(RateFnId, Option<usize>, None);

pub trait InfectiousnessRateFn {
    /// Returns the rate at time `t`
    fn get_rate(&self, t: f64) -> f64;
    /// Returns the maximum rate (useful for rejection sampling)
    fn max_rate(&self) -> f64;
    /// Returns the time at which a person is no longer infectious
    fn max_time(&self) -> f64;
    // fn inverse_cdf(&self, p: f64) -> f64;
}

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
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>);
    fn assign_random_rate_fn(&mut self, person_id: PersonId);
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn;
}

fn get_fn(context: &Context, index: usize) -> &dyn InfectiousnessRateFn {
    context.get_data_container(RateFnPlugin).unwrap().rates[index].as_ref()
}

impl InfectiousnessRateExt for Context {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>) {
        let container = self.get_data_container_mut(RateFnPlugin);
        container.rates.push(dist);
    }
    fn assign_random_rate_fn(&mut self, person_id: PersonId) {
        let max = self.get_data_container_mut(RateFnPlugin).rates.len();
        let index = self.sample_range(InfectiousnessRateRng, 0..max);
        self.set_person_property(person_id, RateFnId, Some(index));
    }
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn {
        let index = self.get_person_property(person_id, RateFnId).unwrap();
        get_fn(self, index)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use ixa::Context;

    // A dummy implementation for testing.
    struct DummyRate {
        constant: f64,
    }

    impl InfectiousnessRateFn for DummyRate {
        fn get_rate(&self, _t: f64) -> f64 {
            self.constant
        }
        fn max_rate(&self) -> f64 {
            self.constant * 2.0
        }
        fn max_time(&self) -> f64 {
            self.constant * 3.0
        }
    }

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_person_rate_fn() {
        let mut context = init_context();
        let person_id = context.add_person(()).unwrap();

        let rate = DummyRate { constant: 1.0 };
        context.add_rate_fn(Box::new(rate));

        context.assign_random_rate_fn(person_id);

        let rate_fn = context.get_person_rate_fn(person_id);

        assert_eq!(rate_fn.get_rate(0.0), 1.0);
        assert_eq!(rate_fn.max_rate(), 2.0);
        assert_eq!(rate_fn.max_time(), 3.0);
    }

    /// Test that if we add two different rate functions, repeated random assignment
    /// eventually assigns both. (We run many assignments and count the distribution.)
    #[test]
    fn test_assign_random_rate_fn_with_multiple_rates() {
        let mut context = init_context();

        let rate1 = DummyRate { constant: 1.0 };
        let rate2 = DummyRate { constant: 2.0 };
        context.add_rate_fn(Box::new(rate1));
        context.add_rate_fn(Box::new(rate2));

        let mut count_rate1 = 0;
        let mut count_rate2 = 0;
        for _ in 0..100 {
            let person_id = context.add_person(()).unwrap();
            context.assign_random_rate_fn(person_id);

            // Retrieve the assigned index via the person property.
            let index = context
                .get_person_property(person_id, RateFnId)
                .expect("Expected a rate index to have been assigned");

            if index == 0 {
                count_rate1 += 1;
            } else if index == 1 {
                count_rate2 += 1;
            } else {
                panic!("Unexpected rate index assigned: {index}");
            }
        }
        assert!(count_rate1 > 0, "Rate function 1 was never assigned");
        assert!(count_rate2 > 0, "Rate function 2 was never assigned");
    }
}
