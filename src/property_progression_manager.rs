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

    use ixa::{Context, ContextPeopleExt, ContextRandomExt, ExecutionPhase};

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
}
