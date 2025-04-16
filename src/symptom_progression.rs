use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    PersonPropertyChangeEvent,
};
use serde::Serialize;
use statrs::distribution::{Exp, Weibull};

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{ContextPropertyProgressionExt, PropertyProgression},
};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum SymptomValue {
    Presymptomatic,
    Category1,
    Category2,
    Category3,
    Category4,
}

define_person_property_with_default!(Symptoms, Option<SymptomValue>, None);

/// Stores information about a symptom progression (presymptomatic -> category{1..=4} -> None).
/// Includes details about the incubation period and time to symptom improvement distributions.
struct SymptomData {
    category: SymptomValue,
    incubation_period: Exp,
    time_to_symptom_improvement: Weibull,
}

impl PropertyProgression for SymptomData {
    type Value = Option<SymptomValue>;
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        // People are `None` until they are infected and after their symptoms have improved.
        if let Some(symptoms) = last {
            // People become presymptomatic when they are infected.
            // If they are presymptomatic, we schedule their symptom development.
            if symptoms == &SymptomValue::Presymptomatic {
                return Some(schedule_symptoms(self, context));
            }
            // Otherwise, person is currently experiencing symptoms, so schedule recovery.
            return Some(schedule_recovery(self, context));
        }
        None
    }
}

fn schedule_symptoms(data: &SymptomData, context: &Context) -> (Option<SymptomValue>, f64) {
    // Draw from the incubation period
    let time = context.sample_distr(SymptomRng, data.incubation_period);
    // Assign this person the corresponding symptom category at the given time
    (Some(data.category), time)
}

fn schedule_recovery(data: &SymptomData, context: &Context) -> (Option<SymptomValue>, f64) {
    // Draw from the time to symptom improvement distribution
    let time = context.sample_distr(SymptomRng, data.time_to_symptom_improvement);
    // Schedule the person to recover from their symptoms (`Symptoms` = `None`) at the given time
    (None, time)
}

pub fn init(context: &mut Context) {
    // To do (kzs9): Read the symptom progression parameters from a file, more broadly think about
    // how we can develop some general infrastructure that allows us to read any distributions in
    // from an external file.
    // For now, we pretend that we have read in a file that has the parameters and we iterate
    // through them.
    // The SymptomData struct lends itself to registering incubation period and time to symptom
    // improvement distributions either for each symptom category or per-person.
    let symptom_categories = [
        SymptomValue::Category1,
        SymptomValue::Category2,
        SymptomValue::Category3,
        SymptomValue::Category4,
    ];
    let incubation_period_parameters = [5.0; 4];
    let time_to_symptom_improvement_parameters = [(2.0, 3.0); 4];

    for idx in 0..symptom_categories.len() {
        let category = symptom_categories[idx];
        let incubation_period = Exp::new(incubation_period_parameters[idx]).unwrap();
        let (shape, scale) = time_to_symptom_improvement_parameters[idx];
        let time_to_symptom_improvement = Weibull::new(shape, scale).unwrap();
        let symptom_data = SymptomData {
            category,
            incubation_period,
            time_to_symptom_improvement,
        };
        context.register_property_progression(Symptoms, symptom_data);
    }

    event_subscriptions(context);
}

fn event_subscriptions(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                context.set_person_property(
                    event.person_id,
                    Symptoms,
                    Some(SymptomValue::Presymptomatic),
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
        symptom_progression::Symptoms,
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
        // The person should be through their symptom progression
        assert!(context.get_person_property(person, Symptoms).is_none());
    }
}
