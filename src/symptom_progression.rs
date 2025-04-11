use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    IxaError, PersonPropertyChangeEvent, PersonId,
};
use serde::Serialize;

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum SymptomValue {
    Category1,
    Category2,
    Category3,
    Category4
}

define_person_property_with_default!(
    ClinicalSymptoms,
    Option<SymptomValue>,
    None
);

pub fn init(context: &mut Context) {
    // Save disease data for a person somewhere after infection
    // If symptomatic, choose one of the categories and make a plan to stop being symptomatic
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                schedule_symptoms(context, event.person_id);
            }
        }
    );
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<ClinicalSymptoms>| {
            if let Some(_category) = event.current  {
                schedule_recovery(context, event.person_id)
            }
        }
    );
}

fn schedule_recovery(context: &mut Context, person: PersonId) {
    // Need to call symptom duration from a data plugin
    let symptom_duration = 10.0;
    context.add_plan(context.get_current_time() + symptom_duration,
        move | context| {
            context.set_person_property(
                person,
                ClinicalSymptoms,
                None 
            );
        }
    );
}

fn schedule_symptoms(context: &mut Context, person: PersonId) {
    // Need to call incubation period from disease data plugin
    let incubation_period = 5.0;
    let category = 
        match context.sample_weighted(SymptomRng, &[0.1,0.4,0.3,0.2]) {
            0 => Ok(SymptomValue::Category1),
            1 => Ok(SymptomValue::Category2),
            2 => Ok(SymptomValue::Category3),
            3 => Ok(SymptomValue::Category4),
            4_usize.. => Err(IxaError::IxaError("Error sampling".to_string())),
        }.unwrap();
    
    context.add_plan(context.get_current_time() + incubation_period,
        move | context| {
            context.set_person_property(
                person,
                ClinicalSymptoms,
                Some(category)                            
            );
        }
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::init;
    use crate::{
        parameters::{GlobalParams, RateFnType},
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
