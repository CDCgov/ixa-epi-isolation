use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    IxaError, PersonPropertyChangeEvent,
};
use serde::Serialize;
use statrs::distribution::Exp;

use crate::{
    clinical_status_manager::{
        ContextPropertyProgressionExt, EmpiricalProgression, PropertyProgression,
    },
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
};

define_rng!(SymptomRng);

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

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum DiseaseSeverityValue {
    Mild,
    Moderate,
    Severe,
    Recovered,
}

define_person_property_with_default!(DiseaseSeverity, Option<DiseaseSeverityValue>, None);

pub struct DiseaseSeverityProgression {
    pub mild_to_moderate: f64,
    pub moderate_to_severe: f64,
    pub mild_time: f64,
    pub moderate_time: f64,
    pub severe_time: f64,
}

impl PropertyProgression for DiseaseSeverityProgression {
    type Value = Option<DiseaseSeverityValue>;
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        match last {
            Some(DiseaseSeverityValue::Mild) => {
                // With some probability, the person moves to moderate, otherwise they recover
                if context.sample_bool(SymptomRng, self.mild_to_moderate) {
                    Some((
                        Some(DiseaseSeverityValue::Moderate),
                        context.sample_distr(SymptomRng, Exp::new(1.0 / self.mild_time).unwrap()),
                    ))
                } else {
                    Some((
                        Some(DiseaseSeverityValue::Recovered),
                        context.sample_distr(SymptomRng, Exp::new(1.0 / self.mild_time).unwrap()),
                    ))
                }
            }
            Some(DiseaseSeverityValue::Moderate) => {
                // With some probability, the person moves to severe, otherwise they recover
                if context.sample_bool(SymptomRng, self.moderate_to_severe) {
                    Some((
                        Some(DiseaseSeverityValue::Severe),
                        context
                            .sample_distr(SymptomRng, Exp::new(1.0 / self.moderate_time).unwrap()),
                    ))
                } else {
                    Some((
                        Some(DiseaseSeverityValue::Recovered),
                        context
                            .sample_distr(SymptomRng, Exp::new(1.0 / self.moderate_time).unwrap()),
                    ))
                }
            }
            Some(DiseaseSeverityValue::Severe) => Some((
                Some(DiseaseSeverityValue::Recovered),
                context.sample_distr(SymptomRng, Exp::new(1.0 / self.severe_time).unwrap()),
            )),
            Some(DiseaseSeverityValue::Recovered) | None => None,
        }
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Todo(kzs9): We will read these progressions from a file from our isolation guidance modeling
    // For now, these are example possible progressions based on our isolation guidance modeling
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
        vec![Some(IsolationGuidanceSymptomValue::NoSymptoms)],
        vec![],
    )?;
    context
        .register_property_progression(IsolationGuidanceSymptom, example_progression_asymptomatic);

    // For disease severity, we register a progression with made up values for now.
    // Todo(kzs9): make these real values set by parameters as we decide how to model symptoms
    let disease_severity_progression = DiseaseSeverityProgression {
        mild_to_moderate: 0.5,
        moderate_to_severe: 0.5,
        mild_time: 1.0,
        moderate_time: 2.0,
        severe_time: 3.0,
    };
    context.register_property_progression(DiseaseSeverity, disease_severity_progression);

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
                context.set_person_property(
                    event.person_id,
                    DiseaseSeverity,
                    Some(DiseaseSeverityValue::Mild),
                );
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::{event_subscriptions, init, DiseaseSeverityValue};
    use crate::{
        clinical_status_manager::{ContextPropertyProgressionExt, EmpiricalProgression},
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, RateFnType},
        rate_fns::load_rate_fns,
        symptom_progression::DiseaseSeverity,
        Params,
    };

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ExecutionPhase,
    };

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
    fn test_disease_progression() {
        let mut context = setup();
        let progression = EmpiricalProgression::new(
            vec![
                Some(DiseaseSeverityValue::Mild),
                Some(DiseaseSeverityValue::Moderate),
            ],
            vec![1.0],
        )
        .unwrap();

        // Test that the infectious --> presymptomatic trigger works and sets off the progression.
        let person = context.add_person(()).unwrap();
        context.register_property_progression(DiseaseSeverity, progression);
        event_subscriptions(&mut context);
        context.infect_person(person, None);
        context.add_plan_with_phase(
            1.0,
            move |context| {
                assert_eq!(
                    Some(DiseaseSeverityValue::Moderate),
                    context.get_person_property(person, DiseaseSeverity)
                );
            },
            ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_init() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        init(&mut context).unwrap();
        context.infect_person(person, None);
        context.execute();
        // The only progression that we know for certainty is the disease severity one.
        assert_eq!(
            Some(DiseaseSeverityValue::Recovered),
            context.get_person_property(person, DiseaseSeverity)
        );
    }
}
