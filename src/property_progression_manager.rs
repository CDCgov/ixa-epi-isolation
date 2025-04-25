use std::{
    any::{Any, TypeId},
    collections::HashMap,
    path::PathBuf,
};

use ixa::{
    define_data_plugin, define_rng, Context, ContextPeopleExt, ContextRandomExt, IxaError,
    PersonId, PersonProperty, PersonPropertyChangeEvent,
};
use serde::Deserialize;

use crate::{parameters::ProgressionLibraryType, symptom_progression::SymptomData};

define_rng!(ProgressionRng);

/// Defines a semi-Markovian method for getting the next value based on the last.
/// `T` is the person property being mapped in the progression.
pub trait Progression<T>
where
    T: PersonProperty + 'static,
{
    /// Returns the next value and the time to the next value given the current value and `Context`.
    fn next(
        &self,
        context: &Context,
        person_id: PersonId,
        last: T::Value,
    ) -> Option<(T::Value, f64)>;
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
        tracer: impl Progression<T> + 'static,
    );
}

impl ContextPropertyProgressionExt for Context {
    fn register_property_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl Progression<T> + 'static,
    ) {
        // Add tracer to data container
        let container = self.get_data_container_mut(PropertyProgressions);
        let progressions = container.progressions.entry(TypeId::of::<T>()).or_default();
        let boxxed_tracer = Box::new(tracer) as Box<dyn Progression<T>>;
        progressions.push(Box::new(boxxed_tracer));
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
                    .downcast_ref::<Box<dyn Progression<T>>>()
                    .unwrap()
                    .as_ref();
                if let Some((next_value, time_to_next)) =
                    tcr.next(context, event.person_id, event.current)
                {
                    let current_time = context.get_current_time();
                    context.add_plan(current_time + time_to_next, move |ctx| {
                        ctx.set_person_property(event.person_id, property, next_value);
                    });
                }
            });
        }
    }
}

pub fn load_progressions(
    context: &mut Context,
    library: Option<ProgressionLibraryType>,
) -> Result<(), IxaError> {
    if let Some(library) = library {
        match library {
            ProgressionLibraryType::EmpiricalFromFile { file } => {
                add_progressions_from_file(context, file)?;
            }
        }
    }
    Ok(())
}

#[derive(Deserialize, PartialEq, Debug)]
enum ProgressionType {
    SymptomData,
    // We need this variant for testing that our CSV deserialization catches mismatched progression
    // types with the same id.
    Unimplemented,
}

#[derive(Deserialize)]
struct ProgressionRecord {
    id: u8,
    progression_type: ProgressionType,
    parameter_name: String,
    parameter_value: f64,
}

fn add_progressions_from_file(context: &mut Context, file: PathBuf) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(file)?;
    let mut reader = reader.deserialize::<ProgressionRecord>();
    // Pop out the first record so we can initialize the trackers
    let record = reader.next().unwrap()?;
    let mut last_id = record.id;
    let mut last_progression_type = record.progression_type;
    let mut parameter_names = vec![record.parameter_name];
    let mut parameters = vec![record.parameter_value];
    for record in reader {
        let record = record?;
        if record.id == last_id {
            // Check if the distribution struct is the same
            if record.progression_type != last_progression_type {
                return Err(IxaError::IxaError(format!(
                    "Progression type mismatch: expected {:?}, got {:?}",
                    last_progression_type, record.progression_type
                )));
            }
            // Add to the current parameter vector
            parameter_names.push(record.parameter_name);
            parameters.push(record.parameter_value);
        } else {
            // Take the last values of times and values and make them into a rate function
            if !parameters.is_empty() {
                match last_progression_type {
                    ProgressionType::SymptomData => {
                        SymptomData::register(context, parameter_names, parameters)?;
                    }
                    ProgressionType::Unimplemented => {}
                }
                last_id = record.id;
                last_progression_type = record.progression_type;
            }
            // Start the new values off
            parameter_names = vec![record.parameter_name];
            parameters = vec![record.parameter_value];
        }
    }
    // Add the last progression in the CSV
    match last_progression_type {
        ProgressionType::SymptomData => SymptomData::register(context, parameter_names, parameters),
        ProgressionType::Unimplemented => Ok(()),
    }
}

#[cfg(test)]
mod test {

    use std::{any::TypeId, path::PathBuf};

    use ixa::{
        define_person_property_with_default, Context, ContextPeopleExt, ContextRandomExt,
        ExecutionPhase, IxaError, PersonId, PersonPropertyChangeEvent,
    };
    use statrs::assert_almost_eq;

    use crate::{
        parameters::ProgressionLibraryType,
        population_loader::Age,
        symptom_progression::{SymptomValue, Symptoms},
    };

    use super::{
        load_progressions, ContextPropertyProgressionExt, Progression, PropertyProgressions,
    };

    struct AgeProgression {
        time_to_next_age: f64,
    }

    // Age takes on u8 values
    impl Progression<Age> for AgeProgression {
        fn next(&self, _context: &Context, _person: PersonId, last: u8) -> Option<(u8, f64)> {
            Some((last + 1, self.time_to_next_age))
        }
    }

    #[test]
    fn test_progression_trait() {
        let progression = AgeProgression {
            time_to_next_age: 1.0,
        };
        let mut context = Context::new();
        // Dummy person because Progression's next allows a person as an argument.
        let person = context.add_person(()).unwrap();
        let (next_value, time_to_next) = progression.next(&context, person, 0).unwrap();
        assert_eq!(next_value, 1);
        assert_almost_eq!(time_to_next, 1.0, 0.0);

        let boxed = Box::new(progression);
        let tcr = boxed.as_ref();
        let (next_value, time_to_next) = tcr.next(&context, person, 0).unwrap();
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
        // Dummy person because Progression's next allows a person as an argument.
        let person = context.add_person(()).unwrap();
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
            .downcast_ref::<Box<dyn Progression<Age>>>()
            .unwrap()
            .as_ref();
        // This age progression has 1.0 time unit to the next age.
        assert_eq!(tcr.next(&context, person, 0).unwrap(), (1, 1.0));
        // Same for the second
        let tcr = progressions[1]
            .downcast_ref::<Box<dyn Progression<Age>>>()
            .unwrap()
            .as_ref();
        // This age progression has 2.0 time units to the next age.
        assert_eq!(tcr.next(&context, person, 0).unwrap(), (1, 2.0));
    }

    define_person_property_with_default!(NumberRunningShoes, u8, 0);

    struct RunningShoesProgression {
        max: u8,
        increase: u8,
        time_to_next: f64,
    }

    impl Progression<NumberRunningShoes> for RunningShoesProgression {
        fn next(&self, _context: &Context, _person_id: PersonId, last: u8) -> Option<(u8, f64)> {
            if last >= self.max {
                return None;
            }
            Some((last + self.increase, self.time_to_next))
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
                // Each 0.5 time units, the number of shoes should increase by 2 until hitting 4.
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
                // Age linearly increases by 1 every 1.0 time units.
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
        max_running_shoes: u8,
    }

    impl Progression<NumberRunningShoes> for ShoesMultiplyProgression {
        fn next(&self, context: &Context, person_id: PersonId, last: u8) -> Option<(u8, f64)> {
            let n = context.get_person_property(person_id, NumberRunningShoes);
            if n >= self.max_running_shoes {
                return None;
            }
            Some((last * self.multiplier, self.time_to_next))
        }
    }

    #[test]
    fn test_multiple_implementations() {
        let mut context = Context::new();
        // Dummy person because Progression's next allows a person as an argument.
        let person = context.add_person(()).unwrap();
        let running_shoes_progression = RunningShoesProgression {
            max: 4,
            increase: 2,
            time_to_next: 0.5,
        };
        let shoes_multiply_progression = ShoesMultiplyProgression {
            multiplier: 2,
            time_to_next: 0.5,
            max_running_shoes: 2,
        };
        context.register_property_progression(NumberRunningShoes, running_shoes_progression);
        context.register_property_progression(NumberRunningShoes, shoes_multiply_progression);
        // We need to put this next section in a drop guard because we want to take &DataContainer
        // and then drop it to mutate context.
        {
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
                .downcast_ref::<Box<dyn Progression<NumberRunningShoes>>>()
                .unwrap()
                .as_ref();
            assert_eq!(tcr.next(&context, person, 0).unwrap(), (2, 0.5));
            // Same for the second
            let tcr = shoes_progressions[1]
                .downcast_ref::<Box<dyn Progression<NumberRunningShoes>>>()
                .unwrap()
                .as_ref();
            assert_eq!(tcr.next(&context, person, 1).unwrap(), (2, 0.5));
        }
        // Show that when we change the person property, the behavior of the progression changes.
        context.set_person_property(person, NumberRunningShoes, 5);
        // Get the tracer back out
        let container = context.get_data_container(PropertyProgressions).unwrap();
        let shoes_progressions = container
            .progressions
            .get(&TypeId::of::<NumberRunningShoes>())
            .unwrap();
        let tcr = shoes_progressions[1]
            .downcast_ref::<Box<dyn Progression<NumberRunningShoes>>>()
            .unwrap()
            .as_ref();
        // Regardless of what we plug in as the last value, we get None because we changed the
        // number of running shoes and the tracer behavior depends on that person property.
        assert!(tcr.next(&context, person, 1).is_none());
        assert!(tcr.next(&context, person, 10).is_none());
    }

    #[test]
    fn test_load_library_none_provided_library_type() {
        let mut context = Context::new();
        load_progressions(&mut context, None).unwrap();
        // Check that we have nothing in the progression data container
        let container = context.get_data_container(PropertyProgressions);
        assert!(container.is_none());
    }

    #[test]
    fn test_progression_type_mismatch() {
        let mut context = Context::new();
        let file = PathBuf::from("./tests/data/progression_type_mismatch.csv");
        // Load the library and check for an error
        let result = load_progressions(
            &mut context,
            Some(ProgressionLibraryType::EmpiricalFromFile { file }),
        );
        let e = result.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Progression type mismatch: expected SymptomData, got Unimplemented"
                );
            }
            Some(ue) => panic!(
                "Expected an error that the the progression types should not match. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, loading the progression library passed with no errors."),
        }
    }

    #[test]
    fn test_load_progression_library() {
        let mut context = Context::new();
        context.init_random(0);
        let person = context.add_person(()).unwrap();
        let file = PathBuf::from("./tests/data/two_symptom_data_progressions.csv");
        // Load the library and check for an error
        load_progressions(
            &mut context,
            Some(ProgressionLibraryType::EmpiricalFromFile { file }),
        )
        .unwrap();
        // Get out the registered progressions.
        let container = context.get_data_container(PropertyProgressions).unwrap();
        let progressions = container
            .progressions
            .get(&TypeId::of::<Symptoms>())
            .unwrap();
        assert_eq!(progressions.len(), 2);
        // Inspect the first progression
        let tcr = progressions[0]
            .downcast_ref::<Box<dyn Progression<Symptoms>>>()
            .unwrap()
            .as_ref();
        // Check that this progression gives us category 2
        assert_eq!(
            tcr.next(&context, person, Some(SymptomValue::Presymptomatic))
                .unwrap()
                .0,
            Some(SymptomValue::Category2)
        );
        // Same for the second
        let tcr = progressions[1]
            .downcast_ref::<Box<dyn Progression<Symptoms>>>()
            .unwrap()
            .as_ref();
        // Check that this progression gives us category 3
        assert_eq!(
            tcr.next(&context, person, Some(SymptomValue::Presymptomatic))
                .unwrap()
                .0,
            Some(SymptomValue::Category3)
        );
    }
}
