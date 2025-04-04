use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    IxaError, PersonPropertyChangeEvent,
};
use serde::Serialize;
use statrs::distribution::Exp;

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{
        ContextPropertyProgressionExt, EmpiricalProgression, PropertyProgression,
    },
};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum SymptomValue {
    Category1,
    Category2,
    Category3,
    Category4,
    Improved,
}

define_person_property_with_default!(
    ClinicalSymptoms,
    Option<SymptomValue>,
    None
);

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Todo(kzs9): We will read these progressions from a file from our isolation guidance modeling
    // For now, these are example possible progressions based on our isolation guidance modeling


    /*
    - Subcribe to an infection event: if person is exposed, then decide whether or not to have symptoms
    - If person will have symptoms, choose a symptom category
    - Once in a symptom category, estimate time to recovery or improvement and schedule recovery
    Parameters:
    - shape and scale x 4 categories
    - probability of developing symptoms and probability of symptom category
     */ 
    
    let progression_cat1 = EmpiricalProgression::new(
        vec![
            Some(SymptomValue::Category1),
            Some(SymptomValue::Improved),
        ],
        vec![8.0],
    )?;
    context.register_property_progression(ClinicalSymptoms, progression_cat1);

    let progression_cat2 = EmpiricalProgression::new(
        vec![
            Some(SymptomValue::Category2),
            Some(SymptomValue::Improved),
        ],
        vec![4.0],
    )?;
    context.register_property_progression(ClinicalSymptoms, progression_cat2);

    let progression_cat3 = EmpiricalProgression::new(
        vec![
            Some(SymptomValue::Category3),
            Some(SymptomValue::Improved),
        ],
        vec![3.0],
    )?;
    context.register_property_progression(ClinicalSymptoms, progression_cat3);

    let progression_cat4 = EmpiricalProgression::new(
        vec![
            Some(SymptomValue::Category4),
            Some(SymptomValue::Improved),
        ],
        vec![1.0],
    )?;
    context.register_property_progression(ClinicalSymptoms, progression_cat4);

    event_subscriptions(context);

    Ok(())
}

fn event_subscriptions(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                let category = 
                match context.sample_weighted(SymptomRng, &[0.1,0.4,0.3,0.2]) {
                    0 => Ok(SymptomValue::Category1),
                    1 => Ok(SymptomValue::Category2),
                    2 => Ok(SymptomValue::Category3),
                    3 => Ok(SymptomValue::Category4),
                    4_usize.. => Err(IxaError::IxaError("Error sampling".to_string())),
                }.unwrap();
                context.set_person_property(
                    event.person_id,
                    ClinicalSymptoms,
                    Some(category),
                );               
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::{event_subscriptions, init};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, RateFnType},
        property_progression_manager::{ContextPropertyProgressionExt, EmpiricalProgression},
        rate_fns::load_rate_fns,
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
}
