use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent,
};
use statrs::distribution::Exp;

use crate::{
    define_setting_category,
    parameters::{ContextParametersExt, HospitalizationParameters, Params},
    population_loader::Age,
    settings::{ContextSettingExt, SettingProperties},
    symptom_progression::{SymptomValue, Symptoms},
};

define_setting_category!(Hospital);
define_person_property_with_default!(Hospitalized, bool, false);

define_rng!(HospitalizationRng);

// pub struct HospitalAgeGroups {
//     pub age_minimum: u8,
//     pub age_maximum: u8,
//     pub probabilty: f64,
// }

pub trait ContextHospitalizationExt {
    fn get_hospital_id(&self) -> usize;
}

impl ContextHospitalizationExt for Context {
    fn get_hospital_id(&self) -> usize {
        0 // Placeholder implementation, assuming everyone goes to the same hospital
    }
}

trait ContextHospitalizationInternalExt {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError>;
    fn setup_hospitalization_event_sequence(
        &mut self,
        hospitalization_parameters: HospitalizationParameters,
    );
    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> bool;
}

impl ContextHospitalizationInternalExt for Context {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError> {
        // Modify the hospitalization status of a person
        // depending on the person property value being set activate or deactivate the hospitalization itinerary
        self.set_person_property(person_id, Hospitalized, hospitalized);
        if hospitalized {
            // let hospital_id = self.get_hospital_id()
            // let itinerary = vec![ItineraryEntry::new(
            //     SettingId::new(Hospital, hospital_id),
            //     1.0,
            // )];
            // self.modify_itinerary(person_id, ItineraryModifiers::ReplaceWith { itinerary })?;
            trace!("Person {person_id} is hospitalized at hospital");
        } else {
            // self.remove_modified_itinerary(person_id)?;
            trace!("Person {person_id} is discharged from the hospital");
        }
        Ok(())
    }
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // evaluate hospitalization risk
        // plan a delay to enter the hospital
        if self.evaluate_hospitalization_risk(person_id, hospitalization_parameters.clone()) {
            // Get the hospital ID for the person                    // Plan the arrival at the hospital
            let exp =
                Exp::new(1.0 / hospitalization_parameters.mean_delay_to_hospitalization).unwrap();
            let delay_to_hospitalization = self.sample_distr(HospitalizationRng, exp);
            trace!(
                "Planning hospital arrival for person {person_id} at {}",
                self.get_current_time() + delay_to_hospitalization
            );
            self.add_plan(
                self.get_current_time() + delay_to_hospitalization,
                move |context| {
                    context.modify_hospitalization(person_id, true).unwrap();
                },
            );
            return Ok(());
        }
        Ok(())
    }
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // select a duration of hospitalization
        // plan to leave the hospital after the duration
        trace!("Planning hospital departure for person {person_id}");
        let exp =
            Exp::new(1.0 / hospitalization_parameters.mean_duration_of_hospitalization).unwrap();
        let hospitalization_duration = self.sample_distr(HospitalizationRng, exp);
        self.add_plan(
            self.get_current_time() + hospitalization_duration,
            move |context| {
                context.modify_hospitalization(person_id, false).unwrap();
            },
        );
        Ok(())
    }

    fn setup_hospitalization_event_sequence(
        &mut self,
        hospitalization_parameters: HospitalizationParameters,
    ) {
        // Clone hospitalization_parameters for each closure to avoid move errors
        let hospitalization_parameters_for_symptoms = hospitalization_parameters.clone();
        let hospitalization_parameters_for_hospitalized = hospitalization_parameters.clone();
        // Subscribe to individuals being presymptomatic to plan if/when they enter the hospital
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<Symptoms>| {
            let hospitalization_parameters = hospitalization_parameters_for_symptoms.clone();
            if let Some(SymptomValue::Presymptomatic) = event.current {
                context
                    .plan_hospital_arrival(event.person_id, hospitalization_parameters.clone())
                    .unwrap();
            }
        });
        // Subscribe to individuals being hospitalized to plan when they leave the hospital
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<Hospitalized>| {
                let hospitalization_parameters =
                    hospitalization_parameters_for_hospitalized.clone();
                if event.current {
                    context
                        .plan_hospital_departure(
                            event.person_id,
                            hospitalization_parameters.clone(),
                        )
                        .unwrap();
                }
            },
        );
    }

    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> bool {
        // Evaluate the risk of hospitalization based on the person's age
        let age = self.get_person_property(person_id, Age);
        if age >= 65 {
            return self.sample_bool(
                HospitalizationRng,
                hospitalization_parameters.probability_by_age[2],
            );
        } else if age >= 18 {
            return self.sample_bool(
                HospitalizationRng,
                hospitalization_parameters.probability_by_age[1],
            );
        }
        self.sample_bool(
            HospitalizationRng,
            hospitalization_parameters.probability_by_age[0],
        )
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context
        .register_setting_category(
            &Hospital,
            SettingProperties {
                alpha: 0.0,
                itinerary_specification: None,
            },
        )
        .unwrap();
    let Params {
        hospitalization_parameters,
        ..
    } = context.get_params();
    if let Some(hospitalization_parameters) = hospitalization_parameters {
        context.setup_hospitalization_event_sequence(hospitalization_parameters.clone());
    } else {
        return Err(IxaError::IxaError("No hospitalization parameters provided. They are required for the hospitalization policy.".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::Hospitalized;
    use crate::{
        parameters::{
            CoreSettingsTypes, GlobalParams, HospitalizationParameters, ItinerarySpecificationType,
            ProgressionLibraryType,
        },
        population_loader::Age,
        rate_fns::load_rate_fns,
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, SettingId, SettingProperties,
            Workplace,
        },
        symptom_progression::{SymptomValue, Symptoms},
        Params,
    };
    use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

    use ixa::{
        define_person_property_with_default, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, PersonPropertyChangeEvent,
    };

    use statrs::assert_almost_eq;

    fn setup_context(
        mean_delay_to_hospitalization: f64,
        mean_duration_of_hospitalization: f64,
        probability_by_age: Vec<f64>,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
            proportion_asymptomatic: 0.1,
            max_time: 100.0,
            symptom_progression_library: Some(ProgressionLibraryType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_symptom_parameters.csv"),
            }),
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::Workplace,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::CensusTract,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
            ]),
            hospitalization_parameters: Some(HospitalizationParameters {
                mean_delay_to_hospitalization,
                mean_duration_of_hospitalization,
                probability_by_age,
                hospital_incidence_report_name: Some("hospital_incidence_report.csv".to_string()),
            }),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        crate::settings::init(&mut context);
        context
    }
    #[test]
    fn test_hospital_event_duration() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset
        // 3. Keep track of hospitalization arrival time
        // 4. Keep track of hospitalization departure time
        // 5. Repeat process for multiple simulations.
        // 6. Assert the means of the hospitalization arrival and departure times are within expected ranges.
        let mean_delay_to_hospitalization = 1.0;
        let mean_duration_of_hospitalization = 5.0;
        let probability_by_age = [1.0, 1.0, 1.0].to_vec();
        let num_sim = 20000;
        let delay_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let duration_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        for seed in 0..num_sim {
            let delay_times_clone = Rc::clone(&delay_times);
            let duration_times_clone = Rc::clone(&duration_times);
            let mut context = setup_context(
                mean_delay_to_hospitalization,
                mean_duration_of_hospitalization,
                probability_by_age.clone(),
            );
            context.init_random(seed);
            let p1 = context.add_person((Age, 1u8)).unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context).unwrap();

            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ];
            context.add_itinerary(p1, itinerary).unwrap();

            context.set_person_property(p1, Symptoms, Some(SymptomValue::Presymptomatic));

            define_person_property_with_default!(SymptomStartTime, f64, 0.0);
            define_person_property_with_default!(HospitalStartTime, f64, 0.0);

            context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(
                move |context, event| {
                    if let Some(SymptomValue::Presymptomatic) = event.current {
                        context.set_person_property(
                            event.person_id,
                            SymptomStartTime,
                            context.get_current_time(),
                        );
                    }
                },
            );

            context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(
                move |context, event| {
                    if event.current {
                        context.set_person_property(
                            event.person_id,
                            HospitalStartTime,
                            context.get_current_time(),
                        );
                        delay_times_clone.borrow_mut().push(
                            context.get_current_time()
                                - context.get_person_property(event.person_id, SymptomStartTime),
                        );
                    } else {
                        duration_times_clone.borrow_mut().push(
                            context.get_current_time()
                                - context.get_person_property(event.person_id, HospitalStartTime),
                        );
                    }
                },
            );

            context.execute();
        }

        #[allow(clippy::cast_precision_loss)]
        let avg_delay = {
            let delays = delay_times.borrow();
            delays.iter().sum::<f64>() / num_sim as f64
        };
        #[allow(clippy::cast_precision_loss)]
        let avg_duration = {
            let durations = duration_times.borrow();
            durations.iter().sum::<f64>() / num_sim as f64
        };
        assert_almost_eq!(avg_delay, mean_delay_to_hospitalization, 0.05);
        assert_almost_eq!(avg_duration, mean_duration_of_hospitalization, 0.05);
    }

    #[test]
    fn test_hospital_age_distribution() {
        // 1. Create three new people one in each age group (0-17, 18-64, 65+)
        // 2. Keep track of if they are hospitalized or not
        // 3. Repeat process for multiple simulations.
        // 4. Assert the proportion of times they are hospitalized is what we expect.
        let mean_delay_to_hospitalization = 1.0;
        let mean_duration_of_hospitalization = 5.0;
        let probability_by_age = [0.25, 0.5, 0.75].to_vec();
        let num_sim = 10000;
        let children_hospital_counter = Rc::new(RefCell::new(0usize));
        let adult_hospital_counter = Rc::new(RefCell::new(0usize));
        let eldery_hospital_counter = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sim {
            let children_hospital_counter_clone = Rc::clone(&children_hospital_counter);
            let adult_hospital_counter_clone = Rc::clone(&adult_hospital_counter);
            let eldery_hospital_counter_clone = Rc::clone(&eldery_hospital_counter);
            let mut context = setup_context(
                mean_delay_to_hospitalization,
                mean_duration_of_hospitalization,
                probability_by_age.clone(),
            );
            context.init_random(seed);
            let p1 = context.add_person((Age, 1u8)).unwrap();
            let p2 = context.add_person((Age, 25u8)).unwrap();
            let p3 = context.add_person((Age, 75u8)).unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context).unwrap();

            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ];
            context.add_itinerary(p1, itinerary.clone()).unwrap();
            context.add_itinerary(p2, itinerary.clone()).unwrap();
            context.add_itinerary(p3, itinerary.clone()).unwrap();

            context.set_person_property(p1, Symptoms, Some(SymptomValue::Presymptomatic));
            context.set_person_property(p2, Symptoms, Some(SymptomValue::Presymptomatic));
            context.set_person_property(p3, Symptoms, Some(SymptomValue::Presymptomatic));

            context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(
                move |context, event| {
                    if event.current {
                        if context.get_person_property(event.person_id, Age) < 18 {
                            *children_hospital_counter_clone.borrow_mut() += 1;
                        } else if context.get_person_property(event.person_id, Age) < 65 {
                            *adult_hospital_counter_clone.borrow_mut() += 1;
                        } else {
                            *eldery_hospital_counter_clone.borrow_mut() += 1;
                        }
                    }
                },
            );

            context.execute();
        }

        #[allow(clippy::cast_precision_loss)]
        let children_hospitalization_rate =
            *children_hospital_counter.borrow() as f64 / num_sim as f64;
        #[allow(clippy::cast_precision_loss)]
        let adult_hospitalization_rate = *adult_hospital_counter.borrow() as f64 / num_sim as f64;
        #[allow(clippy::cast_precision_loss)]
        let eldery_hospitalization_rate = *eldery_hospital_counter.borrow() as f64 / num_sim as f64;
        assert_almost_eq!(children_hospitalization_rate, probability_by_age[0], 0.01);
        assert_almost_eq!(adult_hospitalization_rate, probability_by_age[1], 0.01);
        assert_almost_eq!(eldery_hospitalization_rate, probability_by_age[2], 0.01);
    }
}
