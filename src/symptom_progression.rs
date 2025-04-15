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
    Presymptomatic,
    Category1,
    Category2,
    Category3,
    Category4,
}

define_person_property_with_default!(
    IsolationGuidanceSymptom,
    Option<IsolationGuidanceSymptomValue>,
    None
);

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Todo(kzs9): We will read a library of symptom progressions from a file based on our Stan
    // modeling. That file will contain a set of possible symptom progressions -- a sequence of
    // symptom categories and times. This is a follow-up PR and filed as an issue.
    // For now, we demonstrate how these progressions would be constructed and registered.
    for cat in 0..5 {
        let cat_name = match cat {
            0 => IsolationGuidanceSymptomValue::Presymptomatic,
            1 => IsolationGuidanceSymptomValue::Category1,
            2 => IsolationGuidanceSymptomValue::Category2,
            3 => IsolationGuidanceSymptomValue::Category3,
            4 => IsolationGuidanceSymptomValue::Category4,
            _ => unreachable!(),
        };
        let incubation_period = 4.0;
        let symptom_duration = 5.0;
        let progression = EmpiricalProgression::new(
            vec![
                Some(IsolationGuidanceSymptomValue::Presymptomatic),
                Some(cat_name),
                None,
            ],
            vec![incubation_period, symptom_duration],
        )?;
        context.register_property_progression(IsolationGuidanceSymptom, progression);
    }

    let asymptomatic_progression = EmpiricalProgression::new(
        vec![Some(IsolationGuidanceSymptomValue::Presymptomatic), None],
        vec![4.0],
    )?;
    context.register_property_progression(IsolationGuidanceSymptom, asymptomatic_progression);

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
                    Some(IsolationGuidanceSymptomValue::Presymptomatic),
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
        // `Presymptomatic` at the end of the simulation.
        assert!(context
            .get_person_property(person, IsolationGuidanceSymptom)
            .is_some());
        assert!(
            Some(IsolationGuidanceSymptomValue::Presymptomatic)
                != context.get_person_property(person, IsolationGuidanceSymptom)
        );
    }
}
