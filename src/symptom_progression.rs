use std::collections::HashMap;

use ixa::rand::Rng;
use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};
use statrs::distribution::Weibull;

use crate::interventions::{ContextTransmissionModifierExt, TransmissionModifier};
use crate::natural_history_parameter_manager::ContextNaturalHistoryParameterExt;
use crate::parameters::ContextParametersExt;
use crate::rate_fns::RateFn;
use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    property_progression_manager::{load_progressions, ContextPropertyProgressionExt, Progression},
};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize, Deserialize, Eq, Hash)]
pub enum SymptomValue {
    Presymptomatic,
    Category1,
    Category2,
    Category3,
    Category4,
}

define_person_property_with_default!(Symptoms, Option<SymptomValue>, None);

/// Stores information about a symptom progression (presymptomatic -> category{1..=4} -> None)
/// for a person.
/// Includes an incubation period and the time to symptom improvement distribution.
pub struct SymptomData {
    category: SymptomValue,
    // We need to store a singular value as this progression's incubation period rather than a
    // distribution of allowable values because we have done the random sampling elsewhere: to make
    // the empirical rate function, we had to use an incubation period sample to convert from
    // time since symptom onset (the units of the outputs of our triangle viral load) to time since
    // infection (the units of the empirical rate function). We store that value (read in via the
    // input file) here. The natural history parameter manager ensures that this symptom progression
    // is only used for people who have the rate function that was calculated with this value.
    incubation_period: f64,
    time_to_symptom_improvement: RightTruncatedWeibull,
}

#[derive(Copy, Clone)]
/// A Weibull distribution that is right truncated (probability mass of 0 past a given
/// value).
struct RightTruncatedWeibull {
    shape: f64,
    scale: f64,
    upper_bound: f64,
}

impl RightTruncatedWeibull {
    fn new(shape: f64, scale: f64, upper_bound: f64) -> Result<Self, IxaError> {
        if shape < 0.0 {
            return Err(IxaError::IxaError(
                "Weibull shape must be positive.".to_string(),
            ));
        }
        if scale < 0.0 {
            return Err(IxaError::IxaError(
                "Weibull scale must be positive.".to_string(),
            ));
        }
        if upper_bound < 0.0 {
            return Err(IxaError::IxaError(
                "Upper bound of Weibull distribution must be positive.".to_string(),
            ));
        }
        Ok(Self {
            shape,
            scale,
            upper_bound,
        })
    }
}

impl SymptomData {
    #[allow(clippy::needless_pass_by_value)]
    pub fn register(
        context: &mut Context,
        parameter_names: Vec<String>,
        parameters: Vec<f64>,
    ) -> Result<(), IxaError> {
        // The first parameter is the symptom category name, the next three are the
        // parameters for the incubation period distribution, and the final three are
        // the parameters for the Weibull distribution.
        if parameter_names.len() != 5 {
            return Err(IxaError::IxaError(format!(
                "Parameters should be of length 5, but got {}",
                parameter_names.len()
            )));
        }

        let parameter_dict = parameter_names
            .iter()
            .zip(parameters.iter())
            .map(|(s, &f)| (s.as_str(), f))
            .collect::<HashMap<&str, f64>>();

        // Get out the symptom category name
        let category = match parameter_dict
            .get("Symptom category")
            .ok_or(IxaError::IxaError(
                "No Symptom category provided.".to_string(),
            ))? {
            1.0 => SymptomValue::Category1,
            2.0 => SymptomValue::Category2,
            3.0 => SymptomValue::Category3,
            4.0 => SymptomValue::Category4,
            _ => {
                return Err(IxaError::IxaError(
                    "Symptom category must be between 1 and 4.".to_string(),
                ))
            }
        };

        // Get out the incubation period parameters
        let incubation_period =
            *parameter_dict
                .get("Incubation period")
                .ok_or(IxaError::IxaError(
                    "No incubation period provided.".to_string(),
                ))?;
        if incubation_period < 0.0 {
            return Err(IxaError::IxaError(
                "Incubation period must be positive.".to_string(),
            ));
        }

        // Set up the Weibull distribution
        let shape = *parameter_dict
            .get("Weibull shape")
            .ok_or(IxaError::IxaError(
                "No Weibull shape period provided.".to_string(),
            ))?;
        let scale = *parameter_dict
            .get("Weibull scale")
            .ok_or(IxaError::IxaError(
                "No Weibull scale period provided.".to_string(),
            ))?;
        let upper_bound = *parameter_dict
            .get("Weibull upper bound")
            .ok_or(IxaError::IxaError(
                "No Weibull upper bound provided.".to_string(),
            ))?;

        let time_to_symptom_improvement = RightTruncatedWeibull::new(shape, scale, upper_bound)?;
        let progression = SymptomData {
            category,
            incubation_period,
            time_to_symptom_improvement,
        };
        context.register_property_progression(Symptoms, progression);
        Ok(())
    }
}

impl Progression<Symptoms> for SymptomData {
    fn next(
        &self,
        context: &Context,
        event: PersonPropertyChangeEvent<Symptoms>,
    ) -> Option<(Option<SymptomValue>, f64)> {
        // People are `None` until they are infected and after their symptoms have improved.
        if let Some(symptoms) = event.current {
            // People become presymptomatic when they are infected.
            // If they are presymptomatic, we schedule their symptom development.
            if symptoms == SymptomValue::Presymptomatic {
                return Some(schedule_symptoms(self));
            }
            // Otherwise, person is currently experiencing symptoms, so schedule recovery.
            return Some(schedule_recovery(self, context));
        }
        None
    }
}

fn schedule_symptoms(data: &SymptomData) -> (Option<SymptomValue>, f64) {
    // Assign this person the corresponding symptom category at the given time
    (Some(data.category), data.incubation_period)
}

fn schedule_recovery(data: &SymptomData, context: &Context) -> (Option<SymptomValue>, f64) {
    // Draw from the time to symptom improvement distribution
    let time = context.sample(SymptomRng, |rng| {
        // We draw continuous values from the Weibull even though the parameters were fit from
        // discrete symptom improvement times -- this is because the Weibull as implemented in our
        // Stan model accounts for daily interval censoring, so the parameters retain their meaning
        // in a continuous distribution without needing to adjust them.
        let w = data.time_to_symptom_improvement;
        // Since so little mass is above the typical values of the upper bound in our Weibulls,
        // use rejection sampling (could use inverse transform sampling instead)
        let mut sample = w.upper_bound;
        while sample >= w.upper_bound {
            sample = rng.sample(Weibull::new(w.shape, w.scale).unwrap());
        }
        sample
    });
    // Schedule the person to recover from their symptoms (`Symptoms` = `None`) at the given time
    (None, time)
}

#[derive(Debug)]
struct AsymptomaticInfectiousnessModifier;
impl TransmissionModifier for AsymptomaticInfectiousnessModifier {
    fn get_relative_transmission(&self, context: &Context, person_id: ixa::PersonId) -> f64 {
        let relative_infectiousness = context.get_params().relative_infectiousness_asymptomatics;
        let symptoms = context.get_person_property(person_id, Symptoms);
        match symptoms {
            // Some can be either are Some(Presymptmatic) or Some(Category{1..=4})
            // Presymptomatic individuals are not asymptomatic -- they will eventually develop symptoms
            Some(_) => 1.0,
            // None means person is fully asymptomatic -- will never develop symptoms
            None => relative_infectiousness,
        }
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Load the progressions
    let params = context.get_params();
    load_progressions(context, params.symptom_progression_library.clone())?;

    // Subscribe to infection status change events to assign people symptoms
    subscribe_to_becoming_infected(context);

    // Register that asymptomatics have an infectiousness modifier
    context.register_transmission_modifier_fn(
        InfectionStatusValue::Infectious,
        AsymptomaticInfectiousnessModifier,
    );

    // For isolation guidance, each rate function has a corresponding symptom improvement time
    // distribution, so we enforce a 1:1 relationship between the two.
    context.register_parameter_id_assigner(Symptoms, |context, person_id| {
        context.get_parameter_id(RateFn, person_id)
    })?;
    Ok(())
}

fn subscribe_to_becoming_infected(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                let prop_asymptomatic = context.get_params().proportion_asymptomatic;
                // We parameterize the model in terms of proportion asymptomatic, but we only do
                // something if the person is symptomatic, so hence p = 1.0 - prop_asymptomatic.
                if context.sample_bool(SymptomRng, 1.0 - prop_asymptomatic) {
                    // People who will develop symptoms are first presymptomatic -- so we know that
                    // all presymptomatic individuals will eventually develop symptoms.
                    // Becoming presymptomatic triggers the property progression manager's watcher
                    // to schedule the symptoms and recovery.
                    context.set_person_property(
                        event.person_id,
                        Symptoms,
                        Some(SymptomValue::Presymptomatic),
                    );
                }
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use super::{init, SymptomData, SymptomValue};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        interventions::ContextTransmissionModifierExt,
        parameters::{ContextParametersExt, GlobalParams},
        population_loader::Alive,
        property_progression_manager::Progression,
        rate_fns::load_rate_fns,
        symptom_progression::{
            schedule_recovery, schedule_symptoms, subscribe_to_becoming_infected,
            RightTruncatedWeibull, Symptoms,
        },
        Params,
    };

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError,
        PersonPropertyChangeEvent,
    };
    use statrs::assert_almost_eq;

    fn setup(proportion_asymptomatic: f64) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We add infections manually in the tests that require infections.
            max_time: 100.0,
            proportion_asymptomatic,
            relative_infectiousness_asymptomatics: 0.6,
            ..Default::default()
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
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: 5.0,
            time_to_symptom_improvement: RightTruncatedWeibull::new(2.0, 3.0, 28.0).unwrap(),
        };
        let symptoms = schedule_symptoms(&symptom_data);
        assert_eq!(symptoms.0, Some(SymptomValue::Category1));
        assert!(symptoms.1 > 0.0); // Check that the time to symptoms is positive
                                   // Check that the time to symptoms is equal to the incubation period
        assert_almost_eq!(symptoms.1, symptom_data.incubation_period, 0.0);
    }

    #[test]
    fn test_schedule_recovery() {
        let context = setup(0.0);
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: 5.0,
            time_to_symptom_improvement: RightTruncatedWeibull::new(2.0, 3.0, 28.0).unwrap(),
        };
        let recovery = schedule_recovery(&symptom_data, &context);
        assert_eq!(recovery.0, None);
        assert!(recovery.1 > 0.0); // Check that the time to recovery is positive
                                   // Check that the time to recovery is less than the upper bound of the Weibull distribution
        assert!(recovery.1 < symptom_data.time_to_symptom_improvement.upper_bound);
    }

    #[test]
    fn test_progression_impl_symptom_data() {
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: 5.0,
            time_to_symptom_improvement: RightTruncatedWeibull::new(2.0, 3.0, 28.0).unwrap(),
        };
        let mut context = setup(0.0);
        let person_id = context.add_person(()).unwrap();
        let event_presymptomatic = PersonPropertyChangeEvent {
            person_id,
            current: Some(SymptomValue::Presymptomatic),
            previous: None,
        };
        let presymptomatic_next = symptom_data.next(&context, event_presymptomatic);
        assert_eq!(
            presymptomatic_next.unwrap().0,
            Some(SymptomValue::Category1)
        );
        assert!(presymptomatic_next.unwrap().1 > 0.0); // Check that the time to symptoms is positive
        let event_category1 = PersonPropertyChangeEvent {
            person_id,
            current: Some(SymptomValue::Category1),
            previous: Some(SymptomValue::Presymptomatic),
        };
        let category1_next = symptom_data.next(&context, event_category1);
        assert!(category1_next.unwrap().0.is_none());
        assert!(category1_next.unwrap().1 > 0.0); // Check that the time to recovery is positive
        let event_recovery = PersonPropertyChangeEvent {
            person_id,
            current: None,
            previous: Some(SymptomValue::Category1),
        };
        let none_next = symptom_data.next(&context, event_recovery);
        assert!(none_next.is_none());
    }

    #[test]
    fn test_event_subscriptions() {
        let mut context = setup(0.0);
        let person = context.add_person(()).unwrap();
        subscribe_to_becoming_infected(&mut context);
        context.infect_person(person, None, None, None);
        context.execute();
        // The person should be presymptomatic
        assert_eq!(
            context.get_person_property(person, Symptoms),
            Some(SymptomValue::Presymptomatic)
        );
    }

    #[test]
    fn test_init() {
        let mut context = setup(0.0);
        let person = context.add_person(()).unwrap();
        init(&mut context).unwrap();
        context.infect_person(person, None, None, None);
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
        let assigned_category_clone = assigned_category.clone();
        context.subscribe_to_event(
            move |_context, event: PersonPropertyChangeEvent<Symptoms>| {
                if let Some(symptoms) = event.current {
                    if symptoms == SymptomValue::Presymptomatic {
                        // Person must be coming from None and going to Presymptomatic
                        assert!(event.previous.is_none());
                    } else {
                        assert_eq!(event.previous, Some(SymptomValue::Presymptomatic));
                        *assigned_category_clone.borrow_mut() = Some(symptoms);
                    }
                } else if event.current.is_none() {
                    assert!(event.previous != Some(SymptomValue::Presymptomatic));
                    assert_eq!(event.previous, *assigned_category.borrow());
                }
            },
        );
        context.execute();
    }

    #[test]
    fn test_weibull_error_shape() {
        let w = RightTruncatedWeibull::new(-1.0, 1.0, 1.0);
        let e = w.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Weibull shape must be positive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that Weibull shape must be positive.. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, Weibull creation passed with no errors."),
        }
    }

    #[test]
    fn test_weibull_error_scale() {
        let w = RightTruncatedWeibull::new(1.0, -1.0, 1.0);
        let e = w.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Weibull scale must be positive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that Weibull scale must be positive.. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, Weibull creation passed with no errors."),
        }
    }

    #[test]
    fn test_weibull_error_upper_bound() {
        let w = RightTruncatedWeibull::new(1.0, 1.0, -1.0);
        let e = w.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Upper bound of Weibull distribution must be positive.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that Weibull upper bound must be positive.. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, Weibull creation passed with no errors."),
        }
    }

    #[test]
    fn test_register_vecs_not_right_size() {
        let mut context = setup(0.0);
        let parameter_names = vec!["Category1".to_string(), "Category2".to_string()];
        let parameters = vec![1.0, 2.0];
        let e = SymptomData::register(&mut context, parameter_names, parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Parameters should be of length 5, but got 2".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that parameters should be of length 5. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, registration passed with no errors."),
        }
    }

    #[test]
    fn test_register_error_symptom_category() {
        let mut context = setup(0.0);
        let parameter_names = vec![
            "Symptom category".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let parameters = vec![5.0, 1.0, 2.0, 3.0, 4.0];
        let e = SymptomData::register(&mut context, parameter_names, parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Symptom category must be between 1 and 4.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that symptom category must be between 1 and 4. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, registration passed with no errors."),
        }
    }

    #[test]
    fn test_register_incubation_period_not_positive() {
        let mut context = setup(0.0);
        let parameter_names = vec![
            "Symptom category".to_string(),
            "Incubation period".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let parameters = vec![1.0, -1.0, 2.0, 3.0, 4.0];
        let e = SymptomData::register(&mut context, parameter_names, parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Incubation period must be positive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that incubation period must be positive. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, registration passed with no errors."),
        }
    }

    #[test]
    fn test_register_wrong_param_names() {
        let mut context = setup(0.0);
        let parameter_names = vec![
            "Symptom category".to_string(),
            "Outcubation rate".to_string(),
            "Weibull shape".to_string(),
            "Weibull scale".to_string(),
            "Weibull upper bound".to_string(),
        ];
        let parameters = vec![1.0, 5.0, 2.0, 3.0, 4.0];
        let e = SymptomData::register(&mut context, parameter_names, parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "No incubation period provided.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that no incubation period provided. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, registration passed with no errors."),
        }
    }

    #[test]
    fn test_register_produces_right_symptom_data() {
        let mut context = setup(0.0);
        let parameter_names = vec![
            "Symptom category".to_string(),
            "Incubation period".to_string(),
            "Weibull shape".to_string(),
            "Weibull scale".to_string(),
            "Weibull upper bound".to_string(),
        ];
        let parameters = vec![1.0, 5.0, 2.0, 3.0, 4.0];
        SymptomData::register(&mut context, parameter_names, parameters).unwrap();
        // Check that a person goes through this progression as we would expect
        let person = context.add_person(()).unwrap();
        context.add_plan(0.0, move |ctx| {
            ctx.set_person_property(person, Symptoms, Some(SymptomValue::Presymptomatic));
        });
        context.subscribe_to_event(move |ctx, event: PersonPropertyChangeEvent<Symptoms>| {
            if event.current == Some(SymptomValue::Category1) {
                assert_eq!(event.previous, Some(SymptomValue::Presymptomatic));
                // We set the incubation period to five days
                assert_almost_eq!(ctx.get_current_time(), 5.0, 0.0);
            } else if event.current.is_none() {
                assert_eq!(event.previous, Some(SymptomValue::Category1));
                assert!(ctx.get_current_time() > 5.0);
                // Check that the time to symptom improvement is less than the upper bound of the Weibull distribution
                assert!(ctx.get_current_time() < 5.0 + 4.0);
            }
        });
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_right_proportion_becomes_symptomatic() {
        // If we infect 1000 people 100 times, we should see that the proportion of people
        // who become symptomatic follows a binomial distribution with probability parameter
        // equal to the fraction of people who are symptomatic.
        let mut num_people_symptomatic_total = 0;
        let num_people = 1000;
        let num_sims = 100;
        let symptomatic_rate = 0.4; // 40% of people should be symptomatic
        for seed in 0..num_sims {
            let mut context = setup(1.0 - symptomatic_rate);
            context.init_random(seed);
            // Add our people
            for _ in 0..num_people {
                // 1000 people
                context.add_person(()).unwrap();
            }
            subscribe_to_becoming_infected(&mut context);
            // Infect all of the people -- triggering the event subscriptions if they are symptomatic
            for person in context.query_people((Alive, true)) {
                context.infect_person(person, None, None, None);
            }
            context.execute();
            // Get the number of people in the context who are symptomatic
            let symptomatic_count =
                context.query_people_count((Symptoms, Some(SymptomValue::Presymptomatic)));
            num_people_symptomatic_total += symptomatic_count;
        }
        // Check that the proportion of people who are symptomatic is close to the expected
        // proportion
        assert_almost_eq!(
            num_people_symptomatic_total as f64 / (num_people * num_sims) as f64,
            symptomatic_rate,
            0.01
        );
    }

    #[test]
    fn test_asymptomatic_infectiousness_modifier() {
        // Make a simulation where all people are asymptomatic
        let mut context = setup(1.0);
        // Add a person
        let person_id = context.add_person(()).unwrap();
        // Set up the simulation in line with the properties of symptoms
        init(&mut context).unwrap();
        // Infect the person because infectiousness modifiers on symptoms only impact the infectious state
        context.infect_person(person_id, None, None, None);
        // Check that the person's infectiousness is augmented by the relative infectiousness modifier
        let relative_infectiousness_asymptomatics =
            context.get_params().relative_infectiousness_asymptomatics;
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            relative_infectiousness_asymptomatics,
            0.0
        );

        // Do the same for a symptomatic person

        // Make a simulation where all people are symptomatic
        let mut context = setup(0.0);
        // Add a person
        let person_id = context.add_person(()).unwrap();
        // Set up the simulation in line with the properties of symptoms
        init(&mut context).unwrap();
        // Infect the person because infectiousness modifiers on symptoms only impact the infectious state
        context.infect_person(person_id, None, None, None);
        // Make sure the person has symptoms
        context.set_person_property(person_id, Symptoms, Some(SymptomValue::Presymptomatic));
        // Check that the person's infectiousness is augmented by the relative infectiousness modifier
        assert_almost_eq!(context.get_relative_total_transmission(person_id), 1.0, 0.0);
    }
}
