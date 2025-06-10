use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonId, PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    settings::{
        CensusTract, ContextSettingExt, Home, ItineraryEntry, ItineraryModifiers, SettingId,
    },
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_derived_property!(Symptomatic, Option<bool>, [Symptoms], |data| match data {
    Some(SymptomValue::Presymptomatic) => Some(false),
    Some(
        SymptomValue::Category1
        | SymptomValue::Category2
        | SymptomValue::Category3
        | SymptomValue::Category4,
    ) => Some(true),
    None => None,
});
define_rng!(PolicyRng);

trait ContextIsolationGuidanceInternalExt {
    fn modify_isolation_status(&mut self, person: PersonId, isolation_status: bool);
    fn modify_masking_status(&mut self, person: PersonId, masking_status: bool);
    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
    );
}

impl ContextIsolationGuidanceInternalExt for Context {
    fn modify_isolation_status(&mut self, person: PersonId, isolation_status: bool) {
        if isolation_status {
            let _ = self.modify_itinerary(person, ItineraryModifiers::Include { setting: &Home });
        } else {
            let _ = self.remove_modified_itinerary(person);
        }
        self.set_person_property(person, IsolatingStatus, isolation_status);
    }

    fn modify_masking_status(&mut self, person: PersonId, masking_status: bool) {
        if masking_status {
            // let home_id = context.get_setting_id(person, &Home);
            let isolation_itinerary = vec![
                ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
                ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
            ];
            let _ = self.modify_itinerary(
                person,
                ItineraryModifiers::ReplaceWith {
                    itinerary: isolation_itinerary,
                },
            );
        } else {
            let _ = self.remove_modified_itinerary(person);
        }
        self.set_person_property(person, MaskingStatus, masking_status);
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
    ) {
        self.subscribe_to_event::<PersonPropertyChangeEvent<Symptomatic>>(move |context, event| {
            match event.current {
                Some(false) => (),
                Some(true) => {
                    if context.sample_bool(PolicyRng, uptake_probability) {
                        let t = context.get_current_time();
                        context.add_plan(t + uptake_delay_period, move |context| {
                            if context
                                .get_person_property(event.person_id, Symptoms)
                                .is_some()
                            {
                                context.modify_isolation_status(event.person_id, true);
                                trace!("Person {} is now isolating", event.person_id);
                            }
                        });
                    }
                }
                None => {
                    if context.get_person_property(event.person_id, IsolatingStatus) {
                        context.modify_isolation_status(event.person_id, false);
                        context.modify_masking_status(event.person_id, true);
                        trace!(
                            "Person {} is now masking and no longer isolating",
                            event.person_id
                        );
                        let t = context.get_current_time();
                        context.add_plan(t + post_isolation_duration, move |context| {
                            context.modify_masking_status(event.person_id, false);
                            trace!("Person {} is now no longer masking", event.person_id);
                        });
                    }
                }
            }
        });
    }
}

pub fn init(context: &mut Context) {
    let &Params {
        intervention_policy_parameters,
        facemask_parameters,
        ..
    } = context.get_params();

    if let Some(facemask_parameters) = facemask_parameters {
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                MaskingStatus,
                &[(true, facemask_parameters.facemask_transmission_modifier)],
            )
            .unwrap();
    }

    if let Some(intervention_policy_parameters) = intervention_policy_parameters {
        context.setup_isolation_guidance_event_sequence(
            intervention_policy_parameters.post_isolation_duration,
            intervention_policy_parameters.uptake_probability,
            intervention_policy_parameters.uptake_delay_period,
        );
    } else {
        trace!("No isolation policy parameters provided. Skipping isolation guidance setup.");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use super::init as policy_init;
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            FacemaskParameters, GlobalParams, InterventionPolicyParameters, ProgressionLibraryType,
            RateFnType,
        },
        population_loader::Alive,
        rate_fns::load_rate_fns,
        symptom_progression::{init as symptom_init, SymptomValue, Symptoms},
        Params,
    };
    use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt,
        PersonPropertyChangeEvent,
    };

    use super::{IsolatingStatus, MaskingStatus};

    use statrs::assert_almost_eq;

    fn setup_context(
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
        facemask_transmission_modifier: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: Some(ProgressionLibraryType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_symptom_parameters.csv"),
            }),
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
            intervention_policy_parameters: Some(InterventionPolicyParameters {
                post_isolation_duration,
                uptake_probability,
                uptake_delay_period,
            }),
            facemask_parameters: Some(FacemaskParameters {
                facemask_transmission_modifier,
            }),
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_isolation_guidance_event_sequence() {
        // this test checks that times at which and individual starts and
        // stops the isolating and masking is correct relative to the symptom onset
        // and the intervention policy parameters. We expect an individual to begin isolating
        // an uptake_delay_period after they start presenting with symptoms, and to stop isolating
        // when symptoms end. We expect an individual to start masking when symptoms end and mask
        // for post_isolation_duration days.
        let post_isolation_duration = 5.0;
        let uptake_probability = 1.0;
        let uptake_delay_period = 1.0;
        let facemask_transmission_modifier = 0.5;
        let mut context = setup_context(
            post_isolation_duration,
            uptake_probability,
            uptake_delay_period,
            facemask_transmission_modifier,
        );

        let start_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let end_time_symptoms = Rc::new(RefCell::new(0.0f64));

        let start_time_symptoms_clone1 = Rc::clone(&start_time_symptoms);
        let end_time_symptoms_clone1 = Rc::clone(&end_time_symptoms);
        context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(move |context, event| {
            println!(
                "symptom time: {}, event.current: {:?}",
                context.get_current_time(),
                event.current
            );
            match event.current {
                Some(SymptomValue::Presymptomatic) => (),
                Some(
                    SymptomValue::Category1
                    | SymptomValue::Category2
                    | SymptomValue::Category3
                    | SymptomValue::Category4,
                ) => {
                    *start_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                }
                None => {
                    *end_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                }
            }
        });

        let start_time_symptoms_clone2 = Rc::clone(&start_time_symptoms);
        let end_time_symptoms_clone2 = Rc::clone(&end_time_symptoms);
        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                println!(
                    "isolation time: {}, event.current: {:?}",
                    context.get_current_time(),
                    event.current
                );
                if event.current {
                    assert_almost_eq!(
                        *start_time_symptoms_clone2.borrow() + uptake_delay_period,
                        context.get_current_time(),
                        0.0
                    );
                } else {
                    assert_eq!(
                        context.get_current_time(),
                        *end_time_symptoms_clone2.borrow()
                    );
                }
            },
        );

        let end_time_symptoms_clone3 = Rc::clone(&end_time_symptoms);
        context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
            move |context, event| {
                println!(
                    "masking time: {}, event.current: {:?}",
                    context.get_current_time(),
                    event.current
                );
                if event.current {
                    //assert size of populatin masking equals the size of thep population masking
                    assert_eq!(
                        context.get_current_time(),
                        *end_time_symptoms_clone3.borrow()
                    );
                } else {
                    assert_eq!(
                        context.get_current_time(),
                        *end_time_symptoms_clone3.borrow() + post_isolation_duration
                    );
                }
            },
        );

        let p1 = context.add_person(()).unwrap();
        symptom_init(&mut context).unwrap();
        policy_init(&mut context);
        context.infect_person(p1, None, None, None);
        context.execute();
    }

    #[test]
    fn test_isolation_guidance_uptake_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the uptake_probability parameter.
        // Note this requires an uptake delay period of 0.
        let post_isolation_duration = 5.0;
        let uptake_probability = 1.0;
        let uptake_delay_period = 0.0;
        let facemask_efficacy = 0.5;
        let num_people_isolating = Rc::new(RefCell::new(0usize));

        let num_people = 1000;
        let num_sims = 100;
        for seed in 0..num_sims {
            let num_people_isolating_clone = Rc::clone(&num_people_isolating);
            let mut context = setup_context(
                post_isolation_duration,
                uptake_probability,
                uptake_delay_period,
                facemask_efficacy,
            );
            context.init_random(seed);
            let _first_person: ixa::PersonId = context.add_person(()).unwrap();
            symptom_init(&mut context).unwrap();
            policy_init(&mut context);

            // Add our people
            for _ in 1..num_people {
                context.add_person(()).unwrap();
            }
            // Infect all of the people -- triggering the event subscriptions if they are symptomatic
            for person in context.query_people((Alive, true)) {
                context.infect_person(person, None, None, None);
            }

            context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
                move |_context, event| {
                    if event.current {
                        *num_people_isolating_clone.borrow_mut() += 1;
                    }
                },
            );

            context.execute();
        }
        // Check that the proportion of people who are isolating is close to the expected
        // proportion
        #[allow(clippy::cast_precision_loss)]
        let proportion_isolating =
            num_people_isolating.take() as f64 / (num_people * num_sims) as f64;
        assert_almost_eq!(proportion_isolating, uptake_probability, 0.01);
    }
}
