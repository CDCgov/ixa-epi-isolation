use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    PersonId, PersonPropertyChangeEvent,
};
use serde::Serialize;
use statrs::distribution::{Exp, Weibull};

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{ContextPropertyProgressionExt, Progression},
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

impl Progression for SymptomData {
    type Value = Option<SymptomValue>;
    fn next(
        &self,
        context: &Context,
        _person_id: PersonId,
        last: Self::Value,
    ) -> Option<(Self::Value, f64)> {
        // People are `None` until they are infected and after their symptoms have improved.
        if let Some(symptoms) = last {
            // People become presymptomatic when they are infected.
            // If they are presymptomatic, we schedule their symptom development.
            if symptoms == SymptomValue::Presymptomatic {
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
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use super::{init, SymptomData, SymptomValue};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, ItineraryWriteFnType, RateFnType},
        property_progression_manager::Progression,
        rate_fns::load_rate_fns,
        symptom_progression::{
            event_subscriptions, schedule_recovery, schedule_symptoms, Symptoms,
        },
        Params,
    };

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt,
        PersonPropertyChangeEvent,
    };
    use statrs::distribution::{Exp, Weibull};

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
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_schedule_symptoms() {
        let context = setup();
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: Exp::new(5.0).unwrap(),
            time_to_symptom_improvement: Weibull::new(2.0, 3.0).unwrap(),
        };
        let symptoms = schedule_symptoms(&symptom_data, &context);
        assert_eq!(symptoms.0, Some(SymptomValue::Category1));
        assert!(symptoms.1 > 0.0); // Check that the time to symptoms is positive
    }

    #[test]
    fn test_schedule_recovery() {
        let context = setup();
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: Exp::new(5.0).unwrap(),
            time_to_symptom_improvement: Weibull::new(2.0, 3.0).unwrap(),
        };
        let recovery = schedule_recovery(&symptom_data, &context);
        assert_eq!(recovery.0, None);
        assert!(recovery.1 > 0.0); // Check that the time to recovery is positive
    }

    #[test]
    fn test_progression_impl_symptom_data() {
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: Exp::new(5.0).unwrap(),
            time_to_symptom_improvement: Weibull::new(2.0, 3.0).unwrap(),
        };
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        let presymptomatic_next =
            symptom_data.next(&context, person, Some(SymptomValue::Presymptomatic));
        assert_eq!(
            presymptomatic_next.unwrap().0,
            Some(SymptomValue::Category1)
        );
        assert!(presymptomatic_next.unwrap().1 > 0.0); // Check that the time to symptoms is positive
        let category1_next = symptom_data.next(&context, person, Some(SymptomValue::Category1));
        assert!(category1_next.unwrap().0.is_none());
        assert!(category1_next.unwrap().1 > 0.0); // Check that the time to recovery is positive
        let none_next = symptom_data.next(&context, person, None);
        assert!(none_next.is_none());
    }

    #[test]
    fn test_event_subscriptions() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        event_subscriptions(&mut context);
        context.infect_person(person, None);
        context.execute();
        // The person should be presymptomatic
        assert_eq!(
            context.get_person_property(person, Symptoms),
            Some(SymptomValue::Presymptomatic)
        );
    }

    #[test]
    fn test_init() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        init(&mut context);
        context.infect_person(person, None);
        // At time 0, the person should become presymptomatic (because `event_subscriptions`)
        context.add_plan(0.0, move |ctx| {
            assert_eq!(
                ctx.get_person_property(person, Symptoms),
                Some(SymptomValue::Presymptomatic)
            );
        });
        // At some time, the person should be in one of the symptom categories. They should only
        // ever pass through one of the symptom categories. We don't know which one, because we
        // don't know what property progression the person is assigned.
        let assigned_category: Rc<RefCell<Option<SymptomValue>>> = Rc::new(RefCell::new(None));
        context.subscribe_to_event(move |_, event: PersonPropertyChangeEvent<Symptoms>| {
            if let Some(symptoms) = event.current {
                if symptoms == SymptomValue::Presymptomatic {
                    // Person must be coming from None and going to Presymptomatic
                    assert!(event.previous.is_none());
                } else {
                    assert_eq!(event.previous, Some(SymptomValue::Presymptomatic));
                    *assigned_category.borrow_mut() = Some(symptoms);
                }
            } else if event.current.is_none() {
                assert!(event.previous != Some(SymptomValue::Presymptomatic));
                assert_eq!(event.previous, *assigned_category.borrow());
            }
        });
        context.execute();
    }
}
