use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, HashMap, PersonId, PersonPropertyChangeEvent,
    PluginContext,
};
use serde::{Deserialize, Serialize};
use statrs::distribution::Exp;

use crate::{
    parameters::ContextParametersExt,
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(Hospitalized, bool, false);

define_rng!(HospitalizationRng);

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct HospitalAgeGroups {
    pub min: u8,
    pub probability: f64,
}

#[derive(Default)]
struct HospitalDataContainer {
    age_group_mapping: HashMap<u8, HospitalAgeGroups>,
}

impl HospitalDataContainer {
    fn set_age_group_mapping(&mut self, age_groups: &[HospitalAgeGroups]) {
        for i in 1..=(age_groups.len()) {
            let grp = &age_groups[i - 1];
            let max = if i == age_groups.len() {
                121u8
            } else {
                age_groups[i].min
            };
            for age in grp.min..max {
                self.age_group_mapping.insert(age, *grp);
            }
        }
    }

    fn get_age_group(&self, age: u8) -> &HospitalAgeGroups {
        self.age_group_mapping
            .get(&age)
            .expect("Age group not found")
    }
}

define_data_plugin!(
    HospitalDataPlugin,
    HospitalDataContainer,
    HospitalDataContainer::default()
);

trait ContextHospitalizationInternalExt:
    PluginContext + ContextRandomExt + ContextPeopleExt + ContextParametersExt + ContextRandomExt
{
    fn plan_hospital_arrival(&mut self, person_id: PersonId) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // evaluate hospitalization risk
        // plan a delay to enter the hospital
        let mean_delay_to_hospitalization = self
            .get_params()
            .hospitalization_parameters
            .mean_delay_to_hospitalization;
        let exp = Exp::new(1.0 / mean_delay_to_hospitalization).unwrap();
        let duration = self.sample_distr(HospitalizationRng, exp);
        trace!(
            "Planning hospital arrival for person {person_id} at {}",
            self.get_current_time() + duration
        );
        self.add_plan(self.get_current_time() + duration, move |context| {
            context.set_person_property(person_id, Hospitalized, true);
        });
        Ok(())
    }
    fn plan_hospital_departure(&mut self, person_id: PersonId) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // select a duration of hospitalization
        // plan to leave the hospital after the duration
        trace!("Planning hospital departure for person {person_id}");
        let mean_duration_of_hospitalization = self
            .get_params()
            .hospitalization_parameters
            .mean_duration_of_hospitalization;
        let exp = Exp::new(1.0 / mean_duration_of_hospitalization).unwrap();
        let duration = self.sample_distr(HospitalizationRng, exp);
        self.add_plan(self.get_current_time() + duration, move |context| {
            context.set_person_property(person_id, Hospitalized, false);
        });
        Ok(())
    }

    fn evaluate_hospitalization_risk(&mut self, person_id: PersonId) -> bool {
        // Evaluate the risk of hospitalization using the age group probabilities
        let age = self.get_person_property(person_id, Age);
        let p = self
            .get_data_container_mut(HospitalDataPlugin)
            .get_age_group(age)
            .probability;
        self.sample_bool(HospitalizationRng, p)
    }

    fn set_age_group_mapping(&mut self) {
        let age_groups = self
            .get_params()
            .hospitalization_parameters
            .age_groups
            .clone();

        let container = self.get_data_container_mut(HospitalDataPlugin);
        container.set_age_group_mapping(&age_groups);
    }
    fn setup_hospitalization_event_sequence(&mut self) {
        // Subscribe to individuals being presymptomatic to plan if/when they enter the hospital
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<Symptoms>| {
            if let Some(SymptomValue::Presymptomatic) = event.current {
                if context.evaluate_hospitalization_risk(event.person_id) {
                    context.plan_hospital_arrival(event.person_id).unwrap();
                }
            }
        });
        // Subscribe to individuals being hospitalized to plan when they leave the hospital
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<Hospitalized>| {
                if event.current {
                    context.plan_hospital_departure(event.person_id).unwrap();
                }
            },
        );
    }
}
impl ContextHospitalizationInternalExt for Context {}

pub fn init(context: &mut Context) {
    let hospitalization_parameters = &context.get_params().hospitalization_parameters;
    let initialization_check = hospitalization_parameters
        .age_groups
        .iter()
        .any(|grp| grp.probability > 0.0);
    if initialization_check {
        context.set_age_group_mapping();
        context.setup_hospitalization_event_sequence();
    } else {
        trace!(
            "All hospitalization probabilities are zero. Hospitalizations module is not initialized."
        );
    }
}

#[cfg(test)]
mod test {
    use super::Hospitalized;
    use crate::{
        hospitalizations::HospitalAgeGroups,
        parameters::{GlobalParams, HospitalizationParameters, ProgressionLibraryType},
        population_loader::Age,
        rate_fns::load_rate_fns,
        symptom_progression::{SymptomValue, Symptoms},
        Params,
    };
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use ixa::{
        define_person_property_with_default, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, PersonPropertyChangeEvent,
    };

    use statrs::assert_almost_eq;

    fn setup_context(
        mean_delay_to_hospitalization: f64,
        mean_duration_of_hospitalization: f64,
        age_groups: Vec<HospitalAgeGroups>,
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
            hospitalization_parameters: HospitalizationParameters {
                mean_delay_to_hospitalization,
                mean_duration_of_hospitalization,
                age_groups,
                hospital_incidence_report_name: "hospital_incidence_report.csv".to_string(),
            },
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
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
        let age_groups = [
            HospitalAgeGroups {
                min: 0,
                probability: 1.0,
            },
            HospitalAgeGroups {
                min: 19,
                probability: 1.0,
            },
            HospitalAgeGroups {
                min: 65,
                probability: 1.0,
            },
        ]
        .to_vec();
        let num_sim = 10000;
        let delay_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let duration_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        for seed in 0..num_sim {
            let delay_times_clone = Rc::clone(&delay_times);
            let duration_times_clone = Rc::clone(&duration_times);
            let mut context = setup_context(
                mean_delay_to_hospitalization,
                mean_duration_of_hospitalization,
                age_groups.clone(),
            );
            context.init_random(seed);
            let p1 = context.add_person((Age, 1u8)).unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context);
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
        let age_groups = [
            HospitalAgeGroups {
                min: 0,
                probability: 0.25,
            },
            HospitalAgeGroups {
                min: 19,
                probability: 0.5,
            },
            HospitalAgeGroups {
                min: 65,
                probability: 0.75,
            },
        ]
        .to_vec();
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
                age_groups.clone(),
            );
            context.init_random(seed);
            let p1 = context.add_person((Age, 1u8)).unwrap();
            let p2 = context.add_person((Age, 25u8)).unwrap();
            let p3 = context.add_person((Age, 75u8)).unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context);

            context.set_person_property(p1, Symptoms, Some(SymptomValue::Presymptomatic));
            context.set_person_property(p2, Symptoms, Some(SymptomValue::Presymptomatic));
            context.set_person_property(p3, Symptoms, Some(SymptomValue::Presymptomatic));

            context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(
                move |context, event| {
                    if event.current {
                        let age = context.get_person_property(event.person_id, Age);
                        if age == 1 {
                            *children_hospital_counter_clone.borrow_mut() += 1;
                        } else if age == 25 {
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
        assert_almost_eq!(
            children_hospitalization_rate,
            age_groups[0].probability,
            0.01
        );
        assert_almost_eq!(adult_hospitalization_rate, age_groups[1].probability, 0.01);
        assert_almost_eq!(eldery_hospitalization_rate, age_groups[2].probability, 0.01);
    }
}
