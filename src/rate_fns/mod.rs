use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, Context, ContextPeopleExt,
    ContextRandomExt, PersonId,
};

pub mod constant_rate;
pub use constant_rate::ConstantRate;

define_person_property_with_default!(RateFnId, Option<usize>, None);

pub trait InfectiousnessRateFn {
    /// Returns the rate of infection at time `t`
    ///
    /// E.g., Where t=day, `rate(2.0)` -> 1.0 means that at day 2, the person's
    /// rate of infection is 1 person per day.
    fn rate(&self, t: f64) -> f64;

    /// Returns the expected number of infection events that we expect to happen in the interval 0 -> t
    ///
    /// E.g., Where t=day, `cum_rate(4.0)` -> 8.0 means that we would expect to infect 8 people in the
    /// first four days.
    ///
    /// See `ScaledRateFn` for how to calculate the cumulative rate for an interval starting at
    /// a time other than 0.
    fn cum_rate(&self, t: f64) -> f64;

    /// Returns the expected time, starting at 0, at which `p` people will be infected.
    ///
    /// E.g., Where t=day, `inverse_cum_rate(6.0)` -> 2.0 means that we would expect
    /// that it would take 2 days to infect 6 people
    ///
    /// See `ScaledRateFn` for how to calculate the inverse cumulative rate for an interval starting
    /// at a time other than 0.
    fn inverse_cum_rate(&self, p: f64) -> Option<f64>;
}

/// A utility for scaling and shifting an infectiousness rate function
pub struct ScaledRateFn<'a, T>
where
    T: InfectiousnessRateFn + ?Sized,
{
    pub base: &'a T,
    pub scale: f64,
    pub elapsed: f64,
}

impl<'a, T: ?Sized + InfectiousnessRateFn> ScaledRateFn<'a, T> {
    #[must_use]
    pub fn new(base: &'a T, scale: f64, elapsed: f64) -> Self {
        Self {
            base,
            scale,
            elapsed,
        }
    }
}

impl<T: ?Sized + InfectiousnessRateFn> InfectiousnessRateFn for ScaledRateFn<'_, T> {
    /// Returns the rate of infection at time `t` scaled by a factor of `self.scale`,
    /// and shifted by `self.elapsed`.
    fn rate(&self, t: f64) -> f64 {
        self.base.rate(t + self.elapsed) * self.scale
    }
    /// Returns the cumulative rate for a time interval starting at `self.elapsed`, scaled by a factor
    /// of `self.scale`. For example, say you want to calculate the
    /// interval from 3.0 -> 4.0; you would create a `ScaledRateFn` with an elapsed of 3.0 and
    /// take `cum_rate(1.0)` (the end of the period - the start).
    fn cum_rate(&self, t: f64) -> f64 {
        (self.base.cum_rate(t + self.elapsed) - self.base.cum_rate(self.elapsed)) * self.scale
    }
    /// Returns the expected time, starting at `self.elapsed` by which an expected number of infection
    /// `events` will occur, and sped up by a factor of `self.scale`.
    /// For example, say the current time is 2.1 and you want to calculate the time to infect the
    /// next person (p=1.0). You would create a `ScaledRateFn` with an elapsed of 2.1 and take
    /// `inverse_cum_rate(1.0)`. If you want to speed up the rate by a factor of 2.0 (halve the
    /// expected time to infect that person), you would create a `ScaledRateFn` with a scale of 2.0.
    fn inverse_cum_rate(&self, events: f64) -> Option<f64> {
        let elapsed_cum_rate = self.base.cum_rate(self.elapsed);
        Some(
            self.base
                .inverse_cum_rate(events / self.scale + elapsed_cum_rate)?
                - self.elapsed,
        )
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
        self.get_data_container(RateFnPlugin).unwrap().rates[index].as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use ixa::Context;

    struct TestRateFn;

    // This is totally not real, it's just so we can test the interface
    impl InfectiousnessRateFn for TestRateFn {
        fn rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn cum_rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn inverse_cum_rate(&self, _p: f64) -> Option<f64> {
            Some(1.0)
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

        let rate = TestRateFn {};
        context.add_rate_fn(Box::new(rate));

        context.assign_random_rate_fn(person_id);

        let rate_fn = context.get_person_rate_fn(person_id);

        assert_eq!(rate_fn.rate(0.0), 1.0);
    }

    /// Test that if we add two different rate functions, repeated random assignment
    /// eventually assigns both. (We run many assignments and count the distribution.)
    #[test]
    fn test_assign_random_rate_fn_with_multiple_rates() {
        let mut context = init_context();

        let rate1 = TestRateFn {};
        let rate2 = TestRateFn {};
        context.add_rate_fn(Box::new(rate1));
        context.add_rate_fn(Box::new(rate2));

        let mut count_rate1 = 0;
        let mut count_rate2 = 0;

        for _ in 0..100 {
            let person_id = context.add_person(()).unwrap();
            context.assign_random_rate_fn(person_id);

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

    #[test]
    fn test_scale_rate_fn() {
        let rate_fn = ConstantRate::new(2.0, 5.0);
        let scaled_rate_fn = ScaledRateFn {
            base: &rate_fn,
            scale: 2.0,
            elapsed: 0.0,
        };
        assert_eq!(scaled_rate_fn.rate(0.0), 4.0);
        assert_eq!(scaled_rate_fn.rate(5.0), 4.0);
    }
    #[test]
    fn test_scale_rate_fn_with_elapsed() {
        let rate_fn = ConstantRate::new(2.0, 5.0);
        let scaled_rate_fn = ScaledRateFn {
            base: &rate_fn,
            scale: 2.0,
            elapsed: 3.0,
        };
        assert_eq!(scaled_rate_fn.rate(0.0), 4.0);
        // Since the elapsed is 3.0, the rate at t=4.0 is past the total infectious
        // period (3.0 + 4.0 = 7.0), so the rate is 0.0.
        assert_eq!(scaled_rate_fn.rate(4.0), 0.0);
    }
    #[test]
    fn test_scale_rate_fn_cum_rate() {
        let rate_fn = ConstantRate::new(2.0, 5.0);
        let scaled_rate_fn = ScaledRateFn {
            base: &rate_fn,
            scale: 2.0,
            elapsed: 3.0,
        };
        assert_eq!(scaled_rate_fn.cum_rate(1.0), 4.0);
        assert_eq!(scaled_rate_fn.cum_rate(2.0), 8.0);
        // The cumulative rate for t=3.0 with an elapsed t=3.0 is still
        // only 2 days, since the infectiousness period ends at 5.0
        assert_eq!(scaled_rate_fn.cum_rate(3.0), 8.0);
    }
    #[test]
    fn test_scale_rate_fn_inverse_cum_rate() {
        let rate_fn = ConstantRate::new(2.0, 5.0);
        let scaled_rate_fn = ScaledRateFn {
            base: &rate_fn,
            scale: 2.0,
            elapsed: 3.0,
        };
        assert_eq!(scaled_rate_fn.inverse_cum_rate(4.0), Some(1.0));
        assert_eq!(scaled_rate_fn.inverse_cum_rate(8.0), Some(2.0));
        assert_eq!(scaled_rate_fn.inverse_cum_rate(11.0), None);
    }
}
