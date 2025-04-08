use ixa::{
    define_person_property_with_default, Context, ContextPeopleExt, IxaError,
    PersonPropertyChangeEvent,
};
use serde::Serialize;

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{ContextPropertyProgressionExt, EmpiricalProgression},
};

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum IsolationGuidanceSymptomValue {
    NoSymptoms,
    Category1,
    Category2,
    Category3,
    Category4,
    Improved,
}

define_person_property_with_default!(
    IsolationGuidanceSymptom,
    Option<IsolationGuidanceSymptomValue>,
    None
);

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Todo(kzs9): We will read a library of symptom progressions (no symptoms for an incubation
    // period, some symptom category for Stan-calculated symptom improvement time, then improved)
    // from our isolation guidance Stan modeling. This is a follow-up PR and filed as an issue.
    // For now, I provide a few examples of symptom progressions to show how they will be structured
    let example_progression_cat1 = EmpiricalProgression::new(
        vec![
            Some(IsolationGuidanceSymptomValue::NoSymptoms),
            Some(IsolationGuidanceSymptomValue::Category1),
            Some(IsolationGuidanceSymptomValue::Improved),
        ],
        vec![1.0, 8.0],
    )?;
    context.register_property_progression(IsolationGuidanceSymptom, example_progression_cat1);

    let example_progression_cat2 = EmpiricalProgression::new(
        vec![
            Some(IsolationGuidanceSymptomValue::NoSymptoms),
            Some(IsolationGuidanceSymptomValue::Category2),
            Some(IsolationGuidanceSymptomValue::Improved),
        ],
        vec![2.0, 4.0],
    )?;
    context.register_property_progression(IsolationGuidanceSymptom, example_progression_cat2);

    let example_progression_cat3 = EmpiricalProgression::new(
        vec![
            Some(IsolationGuidanceSymptomValue::NoSymptoms),
            Some(IsolationGuidanceSymptomValue::Category3),
            Some(IsolationGuidanceSymptomValue::Improved),
        ],
        vec![3.0, 3.0],
    )?;
    context.register_property_progression(IsolationGuidanceSymptom, example_progression_cat3);

    let example_progression_cat4 = EmpiricalProgression::new(
        vec![
            Some(IsolationGuidanceSymptomValue::NoSymptoms),
            Some(IsolationGuidanceSymptomValue::Category4),
            Some(IsolationGuidanceSymptomValue::Improved),
        ],
        vec![5.0, 1.0],
    )?;
    context.register_property_progression(IsolationGuidanceSymptom, example_progression_cat4);

    let example_progression_asymptomatic = EmpiricalProgression::new(
        vec![
            Some(IsolationGuidanceSymptomValue::NoSymptoms),
            Some(IsolationGuidanceSymptomValue::Improved),
        ],
        vec![4.0],
    )?;
    context
        .register_property_progression(IsolationGuidanceSymptom, example_progression_asymptomatic);

    event_subscriptions(context);

    Ok(())
}

fn event_subscriptions(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                context.set_person_property(
                    event.person_id,
                    IsolationGuidanceSymptom,
                    Some(IsolationGuidanceSymptomValue::NoSymptoms),
                );
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::init;
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, RateFnType},
        rate_fns::load_rate_fns,
        symptom_progression::{IsolationGuidanceSymptom, IsolationGuidanceSymptomValue},
        Params,
    };

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt};

    fn setup() -> Context {
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
        context
    }

    #[test]
    fn test_init() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        init(&mut context).unwrap();
        context.infect_person(person, None);
        context.execute();
        // The only progression that we know for certainty is that the person will not be
        // `NoSymptoms` at the end of the simulation.
        assert!(context
            .get_person_property(person, IsolationGuidanceSymptom)
            .is_some());
        assert!(
            Some(IsolationGuidanceSymptomValue::NoSymptoms)
                != context.get_person_property(person, IsolationGuidanceSymptom)
        );
    }
}
