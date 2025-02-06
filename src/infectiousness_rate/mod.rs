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
    fn scale_rate(&self, t: f64, offset: f64, factor: f64) -> f64;
    /// Returns the maximum rate (useful for rejection sampling)
    fn max_rate(&self) -> f64;
    /// Returns the time at which a person is no longer infectious
    fn max_time(&self) -> f64;
    fn inverse_cum(&self, p: f64) -> Option<f64>;
    // Internal utility for implementing ScaleRateFn
    fn scale_inverse_cum(&self, p: f64, offset: f64, factor: f64) -> Option<f64>;
}

pub struct ScaledRateFn<'a, T>
where
    T: InfectiousnessRateFn + ?Sized,
{
    base: &'a T,
    factor: f64,
    offset: f64,
}

impl<'a, T: ?Sized + InfectiousnessRateFn> ScaledRateFn<'a, T> {
    pub fn new(base: &'a T, factor: f64, offset: f64) -> Self {
        Self {
            base,
            factor,
            offset,
        }
    }
}

impl<'a, T: ?Sized + InfectiousnessRateFn> InfectiousnessRateFn for ScaledRateFn<'a, T> {
    fn get_rate(&self, t: f64) -> f64 {
        self.base.scale_rate(t, self.offset, self.factor)
    }
    fn scale_rate(&self, t: f64, offset: f64, factor: f64) -> f64 {
        self.base.scale_rate(t, offset, factor * self.factor)
    }
    fn max_rate(&self) -> f64 {
        self.base.max_rate() * self.factor
    }
    fn max_time(&self) -> f64 {
        self.base.max_time()
    }
    fn inverse_cum(&self, p: f64) -> Option<f64> {
        self.base.scale_inverse_cum(p, self.offset, self.factor)
    }
    fn scale_inverse_cum(&self, p: f64, offset: f64, factor: f64) -> Option<f64> {
        self.base.scale_inverse_cum(p, offset, factor * self.factor)
    }
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
        let index = self
            .get_person_property(person_id, RateFnId)
            .unwrap_or_else(|| panic!("No rate function for {person_id}"));
        get_fn(self, index)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::constant_rate::ConstantRate;
    use super::*;
    use ixa::Context;

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_person_rate_fn() {
        let mut context = init_context();
        let person_id = context.add_person(()).unwrap();

        let rate = ConstantRate::new(1.0, 3.0);
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

        let rate1 = ConstantRate::new(1.0, 3.0);
        let rate2 = ConstantRate::new(2.0, 3.0);
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
