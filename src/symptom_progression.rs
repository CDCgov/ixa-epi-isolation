use ixa::{
    define_person_property_with_default, warn, Context, ContextPeopleExt, IxaError,
    PersonPropertyChangeEvent,
};
use serde::Serialize;

use crate::{
    clinical_status_manager::{ContextPropertyProgressionExt, PropertyProgression},
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
};

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum DiseaseSeverityValue {
    Healthy,
    Presymptomatic,
    Asymptomatic,
    Mild,
    Moderate,
}

define_person_property_with_default!(
    DiseaseSeverity,
    DiseaseSeverityValue,
    DiseaseSeverityValue::Healthy
);

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

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Todo(kzs9): We will read these progressions from a file from our isolation guidance modeling
    // For now, these are example possible progressions based on our isolation guidance modeling
    let example_progression_cat1 = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Presymptomatic,
            DiseaseSeverityValue::Moderate,
            DiseaseSeverityValue::Healthy,
        ],
        vec![1.0, 4.0],
    )?;
    context.register_property_progression(DiseaseSeverity, example_progression_cat1);

    let example_progression_cat2 = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Presymptomatic,
            DiseaseSeverityValue::Mild,
            DiseaseSeverityValue::Healthy,
        ],
        vec![2.0, 4.0],
    )?;
    context.register_property_progression(DiseaseSeverity, example_progression_cat2);

    let example_progression_asymptomatic = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Asymptomatic,
            DiseaseSeverityValue::Healthy,
        ],
        vec![2.0],
    )?;
    context.register_property_progression(DiseaseSeverity, example_progression_asymptomatic);

    event_subscriptions(context);

    Ok(())
}

fn event_subscriptions(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                context.set_person_property(
                    event.person_id,
                    DiseaseSeverity,
                    DiseaseSeverityValue::Presymptomatic,
                );
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::{event_subscriptions, DiseaseSeverityValue, EmpiricalProgression};
    use crate::{
        clinical_status_manager::{ContextPropertyProgressionExt, PropertyProgression},
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, RateFnType},
        rate_fns::load_rate_fns,
        symptom_progression::DiseaseSeverity,
        Params,
    };

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError};
    use statrs::assert_almost_eq;

    #[test]
    fn test_disease_progression() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        let progression = EmpiricalProgression::new(
            vec![
                DiseaseSeverityValue::Presymptomatic,
                DiseaseSeverityValue::Asymptomatic,
                DiseaseSeverityValue::Mild,
            ],
            vec![1.0, 2.0],
        )
        .unwrap();
        let initial_state = DiseaseSeverityValue::Presymptomatic;
        let (next_state, time) = progression.next(&context, &initial_state).unwrap();
        assert_eq!(next_state, DiseaseSeverityValue::Asymptomatic);
        assert_almost_eq!(time, 1.0, 0.0);

        let initial_state = DiseaseSeverityValue::Asymptomatic;
        let (next_state, time) = progression.next(&context, &initial_state).unwrap();
        assert_eq!(next_state, DiseaseSeverityValue::Mild);
        assert_almost_eq!(time, 2.0, 0.0);

        let initial_state = DiseaseSeverityValue::Mild;
        let next_state = progression.next(&context, &initial_state);
        assert!(next_state.is_none());

        // Test that the infectious --> presymptomatic trigger works and sets off the progression.
        let person = context.add_person(()).unwrap();
        context.register_property_progression(DiseaseSeverity, progression);
        event_subscriptions(&mut context);
        context.infect_person(person, None);
        context.execute();
        assert_eq!(
            DiseaseSeverityValue::Mild,
            context.get_person_property(person, DiseaseSeverity)
        );
        assert_almost_eq!(context.get_current_time(), 3.0, 0.0);
    }

    #[test]
    fn test_empirical_progression_errors_len() {
        let progression = EmpiricalProgression::new(
            vec![
                DiseaseSeverityValue::Presymptomatic,
                DiseaseSeverityValue::Asymptomatic,
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
