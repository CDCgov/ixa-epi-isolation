use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use ixa::{
    define_data_plugin, define_rng, warn, Context, ContextPeopleExt, ContextRandomExt, IxaError,
    PersonProperty, PersonPropertyChangeEvent,
};

define_rng!(ProgressionRng);

/// Defines a semi-Markovian method for getting the next person property value based on the last.
pub trait PropertyProgression {
    /// The value of the person property that is being tracked by the progression.
    type Value;
    /// Returns the next value and the time to the next value given the current value and `Context`.
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)>;
}

#[derive(Default)]
struct ProgressionsContainer {
    progressions: HashMap<TypeId, Vec<Box<dyn Any>>>,
}

define_data_plugin!(
    Progressions,
    ProgressionsContainer,
    ProgressionsContainer::default()
);

pub trait ContextPropertyProgressionExt {
    /// Registers a method that provides a sequence of person property values and automatically
    /// changes the values of person properties according to that sequence.
    fn register_property_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl PropertyProgression<Value = T::Value> + 'static,
    );
}

impl ContextPropertyProgressionExt for Context {
    fn register_property_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl PropertyProgression<Value = T::Value> + 'static,
    ) {
        // Add tracer to data container
        let container = self.get_data_container_mut(Progressions);
        let progressions = container.progressions.entry(TypeId::of::<T>()).or_default();
        let boxed_tracer = Box::new(tracer) as Box<dyn PropertyProgression<Value = T::Value>>;
        progressions.push(Box::new(boxed_tracer));
        // Subscribe to change events if we have not yet already seen a progression that controls
        // flow for this property
        if progressions.len() == 1 {
            self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<T>| {
                let container = context.get_data_container(Progressions).unwrap();
                let progressions = container.progressions.get(&TypeId::of::<T>()).unwrap();
                // Todo(kzs9): Make this not random but rather we pick the same index as the rate
                // function id/some way of correlation between natural history
                let id = context.sample_range(ProgressionRng, 0..progressions.len());
                let tcr = progressions[id]
                    .downcast_ref::<Box<dyn PropertyProgression<Value = T::Value>>>()
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

/// Holds a sequence of unique states and times between subsequent states for changing a person's
/// person property values accordingly. Since all person properties that have Markovian transitions
/// may want to have their progressions defined empirically, we make this a general struct that can
/// hold any type `T` that implements `PartialEq` and `Copy`.
pub struct EmpiricalProgression<T: PartialEq + Copy> {
    states: Vec<T>,
    time_to_next: Vec<f64>,
}

impl<T: PartialEq + Copy> EmpiricalProgression<T> {
    /// Makes a new `EmpiricalProgression<T>` struct that holds a sequence of states of value `T`.
    /// Assumes values in `states` are unique.
    /// # Errors
    /// - If `states` is not one element longer than `time_to_next`.
    pub fn new(
        states: Vec<T>,
        time_to_next: Vec<f64>,
    ) -> Result<EmpiricalProgression<T>, IxaError> {
        if states.len() != time_to_next.len() + 1 {
            return Err(IxaError::IxaError(
                "Size mismatch: states must be one element longer than time_to_next. Instead, "
                    .to_string()
                    + &format!(
                        "states has length {} and time_to_next has length {}.",
                        states.len(),
                        time_to_next.len()
                    ),
            ));
        }
        warn!(
            "Adding an EmpiricalProgression. At this time, we do not check whether values in
        states are unique."
        );
        Ok(EmpiricalProgression {
            states,
            time_to_next,
        })
    }
}

impl<T: PartialEq + Copy> PropertyProgression for EmpiricalProgression<T> {
    type Value = T;
    fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        let mut iter = self.states.iter().enumerate();
        while let Some((_, status)) = iter.next() {
            if status == last {
                return iter
                    .next()
                    .map(|(i, next)| (*next, self.time_to_next[i - 1]));
            }
        }
        None
    }
}
#[cfg(test)]
mod test {

    use std::any::TypeId;

    use ixa::{
        define_person_property_with_default, Context, ContextPeopleExt, ContextRandomExt,
        ExecutionPhase, IxaError,
    };
    use serde::Serialize;

    use crate::population_loader::Age;

    use super::{
        ContextPropertyProgressionExt, EmpiricalProgression, Progressions, PropertyProgression,
    };

    #[derive(PartialEq, Copy, Clone, Debug, Serialize)]
    pub enum DiseaseSeverityValue {
        Mild,
        Moderate,
        Severe,
    }

    define_person_property_with_default!(DiseaseSeverity, Option<DiseaseSeverityValue>, None);

    struct AgeProgression {
        time_to_next_age: f64,
    }

    impl PropertyProgression for AgeProgression {
        type Value = u8;
        fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
            Some((*last + 1, self.time_to_next_age))
        }
    }

    #[test]
    fn test_register_property_progression_automates_moves() {
        let mut context = Context::new();
        context.init_random(0);
        let symptom_progression = EmpiricalProgression::new(
            vec![
                Some(DiseaseSeverityValue::Mild),
                Some(DiseaseSeverityValue::Moderate),
                Some(DiseaseSeverityValue::Severe),
            ],
            vec![1.0, 2.0],
        )
        .unwrap();
        context.register_property_progression(DiseaseSeverity, symptom_progression);
        context.register_property_progression(
            Age,
            AgeProgression {
                time_to_next_age: 1.0,
            },
        );
        let person_id = context.add_person((Age, 0)).unwrap();
        context.set_person_property(person_id, DiseaseSeverity, Some(DiseaseSeverityValue::Mild));
        context.set_person_property(person_id, Age, 0);
        context.add_plan_with_phase(
            1.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 1);
                let severity = ctx.get_person_property(person_id, DiseaseSeverity);
                assert_eq!(severity, Some(DiseaseSeverityValue::Moderate));
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            2.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 2);
                let severity = ctx.get_person_property(person_id, DiseaseSeverity);
                assert_eq!(severity, Some(DiseaseSeverityValue::Moderate));
            },
            ExecutionPhase::Last,
        );
        context.add_plan_with_phase(
            3.0,
            move |ctx| {
                let age = ctx.get_person_property(person_id, Age);
                assert_eq!(age, 3);
                let severity = ctx.get_person_property(person_id, DiseaseSeverity);
                assert_eq!(severity, Some(DiseaseSeverityValue::Severe));
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
        let progression_to_severe = EmpiricalProgression::new(
            vec![
                Some(DiseaseSeverityValue::Mild),
                Some(DiseaseSeverityValue::Moderate),
                Some(DiseaseSeverityValue::Severe),
            ],
            vec![1.0, 2.0],
        )
        .unwrap();
        let progression_moderate = EmpiricalProgression::new(
            vec![
                Some(DiseaseSeverityValue::Mild),
                Some(DiseaseSeverityValue::Moderate),
            ],
            vec![2.0],
        )
        .unwrap();
        context.register_property_progression(DiseaseSeverity, progression_to_severe);
        context.register_property_progression(DiseaseSeverity, progression_moderate);
        // Get out the registered progressions.
        let container = context.get_data_container(Progressions).unwrap();
        let progressions = container
            .progressions
            .get(&TypeId::of::<DiseaseSeverity>())
            .unwrap();
        assert_eq!(progressions.len(), 2);
        // Inspect the first progression
        let tcr = progressions[0]
            .downcast_ref::<Box<dyn PropertyProgression<Value = Option<DiseaseSeverityValue>>>>()
            .unwrap()
            .as_ref();
        assert_eq!(
            tcr.next(&context, &Some(DiseaseSeverityValue::Mild))
                .unwrap(),
            (Some(DiseaseSeverityValue::Moderate), 1.0)
        );
        assert_eq!(
            tcr.next(&context, &Some(DiseaseSeverityValue::Moderate))
                .unwrap(),
            (Some(DiseaseSeverityValue::Severe), 2.0)
        );
        assert!(tcr
            .next(&context, &Some(DiseaseSeverityValue::Severe))
            .is_none());
        // Same for the second
        let tcr = progressions[1]
            .downcast_ref::<Box<dyn PropertyProgression<Value = Option<DiseaseSeverityValue>>>>()
            .unwrap()
            .as_ref();
        assert_eq!(
            tcr.next(&context, &Some(DiseaseSeverityValue::Mild))
                .unwrap(),
            (Some(DiseaseSeverityValue::Moderate), 2.0)
        );
        assert!(tcr
            .next(&context, &Some(DiseaseSeverityValue::Moderate))
            .is_none());
    }

    #[test]
    fn test_empirical_progression_errors_len() {
        let progression = EmpiricalProgression::new(
            vec![
                DiseaseSeverityValue::Mild,
                DiseaseSeverityValue::Moderate,
                DiseaseSeverityValue::Mild,
            ],
            vec![1.0, 2.0, 3.0],
        );
        let e = progression.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Size mismatch: states must be one element longer than time_to_next. Instead, states has length 3 and time_to_next has length 3.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that states and time_to_next have incompatible sizes. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
