use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use ixa::{
    define_data_plugin, define_rng, Context, ContextPeopleExt, ContextRandomExt, PersonProperty,
    PersonPropertyChangeEvent,
};

define_rng!(ProgressionRng);

/// Defines a semi-Markovian method for getting the next value based on the last.
pub trait Progression {
    /// The value being tracked by the progression: usually a person property'es value.
    type Value;
    /// Returns the next value and the time to the next value given the current value and `Context`.
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)>;
}

#[derive(Default)]
struct PropertyProgressionsContainer {
    progressions: HashMap<TypeId, Vec<Box<dyn Any>>>,
}

define_data_plugin!(
    PropertyProgressions,
    PropertyProgressionsContainer,
    PropertyProgressionsContainer::default()
);

pub trait ContextPropertyProgressionExt {
    /// Registers a method that provides a sequence of person property values and times, and
    /// automatically changes the values of person properties according to that sequence.
    fn register_property_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl Progression<Value = T::Value> + 'static,
    );
}

impl ContextPropertyProgressionExt for Context {
    fn register_property_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl Progression<Value = T::Value> + 'static,
    ) {
        // Add tracer to data container
        let container = self.get_data_container_mut(PropertyProgressions);
        let progressions = container.progressions.entry(TypeId::of::<T>()).or_default();
        let boxed_tracer = Box::new(tracer) as Box<dyn Progression<Value = T::Value>>;
        progressions.push(Box::new(boxed_tracer));
        // Subscribe to change events if we have not yet already seen a progression that controls
        // flow for this property
        if progressions.len() == 1 {
            self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<T>| {
                let container = context.get_data_container(PropertyProgressions).unwrap();
                let progressions = container.progressions.get(&TypeId::of::<T>()).unwrap();
                // Todo(kzs9): Make this not random but rather we pick the same index as the rate
                // function id/some way of correlation between natural history
                let id = context.sample_range(ProgressionRng, 0..progressions.len());
                let tcr = progressions[id]
                    .downcast_ref::<Box<dyn Progression<Value = T::Value>>>()
                    .unwrap()
                    .as_ref();
                if let Some((next_value, time_to_next)) = tcr.next(context, &event.current) {
                    let current_time = context.get_current_time();
                    context.add_plan(current_time + time_to_next, move |ctx| {
                        ctx.set_person_property(event.person_id, property, next_value);
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod test {

    use std::any::TypeId;

    use ixa::{
        define_person_property_with_default, Context, ContextPeopleExt, ContextRandomExt,
        ExecutionPhase, PersonPropertyChangeEvent,
    };
    use statrs::assert_almost_eq;

    use crate::population_loader::Age;

    use super::{ContextPropertyProgressionExt, Progression, PropertyProgressions};

    struct AgeProgression {
        time_to_next_age: f64,
    }

    impl Progression for AgeProgression {
        type Value = u8;
        fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
            Some((*last + 1, self.time_to_next_age))
        }
    }

    #[test]
    fn test_progression_trait() {
        let progression = AgeProgression {
            time_to_next_age: 1.0,
        };
        let context = Context::new();
        let (next_value, time_to_next) = progression.next(&context, &0).unwrap();
        assert_eq!(next_value, 1);
        assert_almost_eq!(time_to_next, 1.0, 0.0);

        let boxed = Box::new(progression) as Box<dyn Progression<Value = u8>>;
        let tcr = boxed.as_ref();
        let (next_value, time_to_next) = tcr.next(&context, &0).unwrap();
        assert_eq!(next_value, 1);
        assert_almost_eq!(time_to_next, 1.0, 0.0);
    }

    #[test]
    fn test_register_property_progression_automates_moves() {
        let mut context = Context::new();
        context.init_random(0);
        context.register_property_progression(
            Age,
            AgeProgression {
                time_to_next_age: 1.0,
            },
        );
        let person_id = context.add_person((Age, 0)).unwrap();
        context.set_person_property(person_id, Age, 0);
        context.add_plan_with_phase(
            1.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 1);
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            2.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 2);
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            3.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 3);
            },
            ExecutionPhase::Last,
        );
        // Since age increases never stop (just keep on +1-ing), we need a plan to shutdown context.
        context.add_plan_with_phase(3.0, Context::shutdown, ExecutionPhase::Last);
        context.execute();
    }

    #[test]
    fn test_multiple_progressions_registered() {
        let mut context = Context::new();
        context.init_random(0);
        let one_yr_progression = AgeProgression {
            time_to_next_age: 1.0,
        };
        let two_yr_progression = AgeProgression {
            time_to_next_age: 2.0,
        };
        context.register_property_progression(Age, one_yr_progression);
        context.register_property_progression(Age, two_yr_progression);
        // Get out the registered progressions.
        let container = context.get_data_container(PropertyProgressions).unwrap();
        let progressions = container.progressions.get(&TypeId::of::<Age>()).unwrap();
        assert_eq!(progressions.len(), 2);
        // Inspect the first progression
        let tcr = progressions[0]
            .downcast_ref::<Box<dyn Progression<Value = u8>>>()
            .unwrap()
            .as_ref();
        assert_eq!(tcr.next(&context, &0_u8).unwrap(), (1_u8, 1.0));
        // Same for the second
        let tcr = progressions[1]
            .downcast_ref::<Box<dyn Progression<Value = u8>>>()
            .unwrap()
            .as_ref();
        assert_eq!(tcr.next(&context, &0_u8).unwrap(), (1_u8, 2.0));
    }

    define_person_property_with_default!(NumberRunningShoes, u8, 0);

    struct RunningShoesProgression {
        max: u8,
        increase: u8,
        time_to_next: f64,
    }

    impl Progression for RunningShoesProgression {
        type Value = u8;
        fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
            if *last >= self.max {
                return None;
            }
            Some((*last + self.increase, self.time_to_next))
        }
    }

    #[test]
    fn test_multiple_properties_registered() {
        let mut context = Context::new();
        context.init_random(0);
        let age_progression = AgeProgression {
            time_to_next_age: 1.0,
        };
        let shoe_progression = RunningShoesProgression {
            max: 4,
            increase: 2,
            time_to_next: 0.5,
        };
        context.register_property_progression(Age, age_progression);
        context.register_property_progression(NumberRunningShoes, shoe_progression);
        // Get out the registered progressions.
        let container = context.get_data_container(PropertyProgressions).unwrap();
        let age_progressions = container.progressions.get(&TypeId::of::<Age>()).unwrap();
        assert_eq!(age_progressions.len(), 1);
        let shoes_progressions = container
            .progressions
            .get(&TypeId::of::<NumberRunningShoes>())
            .unwrap();
        assert_eq!(shoes_progressions.len(), 1);
        // See that the progressions do what they are supposed to do.
        let person = context.add_person((Age, 0)).unwrap();
        context.set_person_property(person, Age, 0);
        context.set_person_property(person, NumberRunningShoes, 0);
        // Number of running shoes increase by 2 every 0.5 time units until greater than 4.
        context.add_plan_with_phase(
            0.5,
            move |ctx| {
                let n = ctx.get_person_property(person, NumberRunningShoes);
                assert_eq!(n, 2);
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            1.0,
            move |ctx| {
                let n = ctx.get_person_property(person, NumberRunningShoes);
                assert_eq!(n, 4);
            },
            ExecutionPhase::Last,
        );
        // There should never ever be any more edits to the number of running shoes beyond 4.
        context.subscribe_to_event(
            move |_, event: PersonPropertyChangeEvent<NumberRunningShoes>| {
                assert!(event.current <= 4);
            },
        );
        context.add_plan_with_phase(
            1.0,
            move |ctx| {
                let age = ctx.get_person_property(person, Age);
                assert_eq!(age, 1);
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            2.0,
            move |ctx| {
                let age = ctx.get_person_property(person, Age);
                assert_eq!(age, 2);
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(3.0, Context::shutdown, ExecutionPhase::Last);
        context.execute();
    }

    struct ShoesMultiplyProgression {
        multiplier: u8,
        time_to_next: f64,
    }

    impl Progression for ShoesMultiplyProgression {
        type Value = u8;
        fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
            Some((*last * self.multiplier, self.time_to_next))
        }
    }

    #[test]
    fn test_multiple_implementations() {
        let mut context = Context::new();
        let running_shoes_progression = RunningShoesProgression {
            max: 4,
            increase: 2,
            time_to_next: 0.5,
        };
        let shoes_multiply_progression = ShoesMultiplyProgression {
            multiplier: 2,
            time_to_next: 0.5,
        };
        context.register_property_progression(NumberRunningShoes, running_shoes_progression);
        context.register_property_progression(NumberRunningShoes, shoes_multiply_progression);
        // Get out the registered progressions.
        let container = context.get_data_container(PropertyProgressions).unwrap();
        let shoes_progressions = container
            .progressions
            .get(&TypeId::of::<NumberRunningShoes>())
            .unwrap();
        assert_eq!(shoes_progressions.len(), 2);
        // See that the progressions do what they are supposed to do even though they are different
        // structs -- in other words, what matters is that they implement the same trait (`Progression`).
        // Inspect the first progression
        let tcr = shoes_progressions[0]
            .downcast_ref::<Box<dyn Progression<Value = u8>>>()
            .unwrap()
            .as_ref();
        assert_eq!(tcr.next(&context, &0_u8).unwrap(), (2_u8, 0.5));
        // Same for the second
        let tcr = shoes_progressions[1]
            .downcast_ref::<Box<dyn Progression<Value = u8>>>()
            .unwrap()
            .as_ref();
        assert_eq!(tcr.next(&context, &1_u8).unwrap(), (2_u8, 0.5));
    }
}
