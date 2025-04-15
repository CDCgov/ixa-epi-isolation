use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    PersonPropertyChangeEvent,
};
use serde::Serialize;

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{ContextPropertyProgressionExt, PropertyProgression},
};

define_rng!(SymptomRng);

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

struct IsolationGuidanceSymptomProgression {
    symptom_category_weights: Vec<f64>,
    // We can generalize these to Vec<f64> or Vec<Distributions> to allow for category-specific
    // distributions
    incubation_period: f64,
    time_to_symptom_improvement: f64,
}

impl PropertyProgression for IsolationGuidanceSymptomProgression {
    type Value = Option<IsolationGuidanceSymptomValue>;

    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        if let Some(symptoms) = last {
            if symptoms == &IsolationGuidanceSymptomValue::Presymptomatic {
                return Some(schedule_symptoms(self, context));
            }
            return Some(schedule_recovery(self));
        }
        None
    }
}

fn schedule_symptoms(
    progression: &IsolationGuidanceSymptomProgression,
    context: &Context,
) -> (Option<IsolationGuidanceSymptomValue>, f64) {
    let category = match context.sample_weighted(SymptomRng, &progression.symptom_category_weights)
    {
        0 => IsolationGuidanceSymptomValue::Category1,
        1 => IsolationGuidanceSymptomValue::Category2,
        2 => IsolationGuidanceSymptomValue::Category3,
        3 => IsolationGuidanceSymptomValue::Category4,
        _ => unreachable!(),
    };
    (Some(category), progression.incubation_period)
}

fn schedule_recovery(
    progression: &IsolationGuidanceSymptomProgression,
) -> (Option<IsolationGuidanceSymptomValue>, f64) {
    let time_to_symptom_improvement = progression.time_to_symptom_improvement;
    (None, time_to_symptom_improvement)
}

pub fn init(context: &mut Context) {
    let isolation_guidance_symptom_progression = IsolationGuidanceSymptomProgression {
        symptom_category_weights: vec![0.25, 0.25, 0.25, 0.25],
        incubation_period: 1.0,
        time_to_symptom_improvement: 1.0,
    };

    context.register_property_progression(
        IsolationGuidanceSymptom,
        isolation_guidance_symptom_progression,
    );

    event_subscriptions(context);
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
        symptom_progression::IsolationGuidanceSymptom,
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
        init(&mut context);
        context.infect_person(person, None);
        context.execute();
        assert!(context
            .get_person_property(person, IsolationGuidanceSymptom)
            .is_none());
    }
}
