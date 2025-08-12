use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent, PluginContext,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    policies::Policies,
    settings::{ContextSettingExt, Home, ItineraryModifiers},
    symptom_progression::PresentingWithSymptoms,
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);

define_rng!(PolicyRng);

#[derive(Debug, Clone, Copy)]
struct InterventionPolicyParameters {
    post_isolation_duration: f64,
    isolation_probability: f64,
    isolation_delay_period: f64,
}

trait ContextIsolationGuidanceInternalExt:
    PluginContext + ContextRandomExt + ContextPeopleExt + ContextSettingExt
{
    fn modify_isolation_status(
        &mut self,
        person: PersonId,
        isolation_status: bool,
    ) -> Result<(), IxaError> {
        if self.get_person_property(person, IsolatingStatus) != isolation_status {
            self.set_person_property(person, IsolatingStatus, isolation_status);
            if isolation_status {
                self.modify_itinerary(person, ItineraryModifiers::RestrictTo { setting: &Home })?;
            } else {
                self.remove_modified_itinerary(person)?;
            }
        }
        Ok(())
    }

    fn isolation(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if self.sample_bool(
            PolicyRng,
            intervention_policy_parameters.isolation_probability,
        ) {
            self.add_plan(
                self.get_current_time() + intervention_policy_parameters.isolation_delay_period,
                move |context| {
                    if context.get_person_property(person_id, PresentingWithSymptoms) {
                        context.modify_isolation_status(person_id, true).unwrap();
                        trace!("Person {person_id} is now isolating");
                    }
                },
            );
        }
    }

    fn post_isolation(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if self.get_person_property(person_id, IsolatingStatus) {
            self.modify_isolation_status(person_id, false).unwrap();
            self.set_person_property(person_id, MaskingStatus, true);
            trace!("Person {person_id} is now masking and no longer isolating");
            self.add_plan(
                self.get_current_time() + intervention_policy_parameters.post_isolation_duration,
                move |context| {
                    context.set_person_property(person_id, MaskingStatus, false);
                    trace!("Person {person_id} is now no longer masking");
                },
            );
        }
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<PresentingWithSymptoms>| {
                if event.current {
                    context.isolation(event.person_id, intervention_policy_parameters);
                } else if event.previous {
                    //individuals transition from presenting with symptoms to not presenting with symptoms
                    context.post_isolation(event.person_id, intervention_policy_parameters);
                }
            },
        );
    }
}
impl ContextIsolationGuidanceInternalExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        guidance_policy,
        facemask_parameters,
        ..
    } = context.get_params();

    if let Some(facemask_parameters) = facemask_parameters {
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                MaskingStatus,
                &[(true, 1.0 - facemask_parameters.facemask_efficacy)],
            )
            .unwrap();
    } else {
        return Err(IxaError::IxaError(
            "No facemask parameters provided. They are required for the intervention policy."
                .to_string(),
        ));
    }

    match guidance_policy {
        Some(Policies::UpdatedIsolationGuidance {
            post_isolation_duration,
            isolation_probability,
            isolation_delay_period,
        }) => {
            let intervention_policy_parameters = InterventionPolicyParameters {
                post_isolation_duration,
                isolation_probability,
                isolation_delay_period,
            };
            context.setup_isolation_guidance_event_sequence(intervention_policy_parameters);
        }
        _ => {
            return Err(IxaError::IxaError(
                "Policy enum does not match specified enum variant for updated guidance."
                    .to_string(),
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::PresentingWithSymptoms;
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            CoreSettingsTypes, FacemaskParameters, GlobalParams, ItinerarySpecificationType,
            ProgressionLibraryType,
        },
        policies::Policies,
        population_loader::Alive,
        rate_fns::load_rate_fns,
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, SettingId, SettingProperties,
            Workplace,
        },
        Params,
    };
    use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

    use ixa::{
        define_person_property_with_default, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, IxaError, PersonPropertyChangeEvent,
    };

    use super::{IsolatingStatus, MaskingStatus};

    use statrs::assert_almost_eq;

    fn setup_context(
        post_isolation_duration: f64,
        isolation_probability: f64,
        isolation_delay_period: f64,
        facemask_efficacy: f64,
        proportion_asymptomatic: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
            proportion_asymptomatic,
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
            guidance_policy: Some(Policies::UpdatedIsolationGuidance {
                post_isolation_duration,
                isolation_probability,
                isolation_delay_period,
            }),
            facemask_parameters: Some(FacemaskParameters { facemask_efficacy }),
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        crate::settings::init(&mut context);
        context
    }

    #[test]
    fn test_isolation_guidance_event_sequence() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset and duration
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that end of isolation is end of symptoms
        // 5. Assert that start of facemask is end of symptoms
        // 6. Assert that end of facemask is end of symptoms + post isolation days
        let post_isolation_duration = 5.0;
        let isolation_probability = 1.0;
        let isolation_delay_period = 1.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.0;

        let mut context = setup_context(
            post_isolation_duration,
            isolation_probability,
            isolation_delay_period,
            facemask_efficacy,
            proportion_asymptomatic,
        );
        let p1 = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];
        context.add_itinerary(p1, itinerary).unwrap();
        crate::symptom_progression::init(&mut context).unwrap();
        super::init(&mut context).unwrap();

        let policy_ran_flag = Rc::new(RefCell::new(false));
        let policy_ran_flag_clone = Rc::clone(&policy_ran_flag);

        define_person_property_with_default!(SymptomStartTime, f64, 0.0);
        define_person_property_with_default!(SymptomEndTime, f64, 0.0);

        context.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                if event.current {
                    context.set_person_property(
                        event.person_id,
                        SymptomStartTime,
                        context.get_current_time(),
                    );
                } else if event.previous {
                    context.set_person_property(
                        event.person_id,
                        SymptomEndTime,
                        context.get_current_time(),
                    );
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                if event.current {
                    assert_almost_eq!(
                        context.get_person_property(event.person_id, SymptomStartTime)
                            + isolation_delay_period,
                        context.get_current_time(),
                        0.0
                    );
                } else {
                    assert_almost_eq!(
                        context.get_person_property(event.person_id, SymptomEndTime),
                        context.get_current_time(),
                        0.0
                    );
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
            move |context, event| {
                if event.current {
                    //assert size of populatin masking equals the size of thep population masking
                    assert_almost_eq!(
                        context.get_person_property(event.person_id, SymptomEndTime),
                        context.get_current_time(),
                        0.0
                    );
                } else {
                    assert_almost_eq!(
                        context.get_person_property(event.person_id, SymptomEndTime)
                            + post_isolation_duration,
                        context.get_current_time(),
                        0.0
                    );
                    *policy_ran_flag_clone.borrow_mut() = true;
                }
            },
        );
        context.infect_person(p1, None, None, None);
        context.execute();

        // Check that the policy ran
        assert!(*policy_ran_flag.borrow());
    }

    #[test]
    fn test_isolation_guidance_event_sequence_verison_two() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset and duration
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that end of isolation is end of symptoms
        // 5. Assert that start of facemask is end of symptoms
        // 6. Assert that end of facemask is end of symptoms + post isolation days
        let post_isolation_duration = 5.0;
        let isolation_probability = 1.0;
        let isolation_delay_period = 1.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.0;

        let mut context = setup_context(
            post_isolation_duration,
            isolation_probability,
            isolation_delay_period,
            facemask_efficacy,
            proportion_asymptomatic,
        );
        let p1 = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];
        context.add_itinerary(p1, itinerary).unwrap();
        crate::symptom_progression::init(&mut context).unwrap();
        super::init(&mut context).unwrap();

        let symptom_start_time = Rc::new(RefCell::new(0.0));
        let symptom_start_time_clone = Rc::clone(&symptom_start_time);
        let symptom_end_time = Rc::new(RefCell::new(0.0));
        let symptom_end_time_clone = Rc::clone(&symptom_end_time);
        let isolation_start_time = Rc::new(RefCell::new(0.0));
        let isolation_start_time_clone = Rc::clone(&isolation_start_time);
        let isolation_end_time = Rc::new(RefCell::new(0.0));
        let isolation_end_time_clone = Rc::clone(&isolation_end_time);
        let masking_start_time = Rc::new(RefCell::new(0.0));
        let masking_start_time_clone = Rc::clone(&masking_start_time);
        let masking_end_time = Rc::new(RefCell::new(0.0));
        let masking_end_time_clone = Rc::clone(&masking_end_time);


        context.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                if event.current {
                    *symptom_start_time_clone.borrow_mut() = context.get_current_time();
                } else if event.previous {
                    *symptom_end_time_clone.borrow_mut() = context.get_current_time();
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                if event.current {
                    *isolation_start_time_clone.borrow_mut() = context.get_current_time();
                } else {
                    *isolation_end_time_clone.borrow_mut() = context.get_current_time();
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
            move |context, event| {
                if event.current {
                    //assert size of populatin masking equals the size of thep population masking
                    *masking_start_time_clone.borrow_mut() = context.get_current_time();
                } else {
                    *masking_end_time_clone.borrow_mut() = context.get_current_time();
                }
            },
        );
        context.infect_person(p1, None, None, None);
        context.execute();

        assert_almost_eq!(
            *symptom_start_time.borrow() + isolation_delay_period,
            *isolation_start_time.borrow(),
            0.0
        );
        assert_almost_eq!(
            *symptom_end_time.borrow(),
            *isolation_end_time.borrow(),
            0.0
        );
        assert_almost_eq!(
            *symptom_end_time.borrow(),
            *masking_start_time.borrow(),
            0.0
        );
        assert_almost_eq!(
            *symptom_end_time.borrow() + post_isolation_duration,
            *masking_end_time.borrow(),
            0.0
        );
    }

    #[test]
    fn test_isolation_guidance_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the isolation probability parameter.
        // Note this requires an isolation delay period of 0.

        let post_isolation_duration = 5.0;
        let isolation_probability = 0.75;
        let isolation_delay_period = 0.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.3;

        let num_people_isolating = Rc::new(RefCell::new(0usize));

        let num_people = 1000;
        let num_sims = 100;
        for seed in 0..num_sims {
            let num_people_isolating_clone = Rc::clone(&num_people_isolating);
            let mut context = setup_context(
                post_isolation_duration,
                isolation_probability,
                isolation_delay_period,
                facemask_efficacy,
                proportion_asymptomatic,
            );
            context.init_random(seed);
            let first_person = context.add_person(()).unwrap();
            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ];
            context
                .add_itinerary(first_person, itinerary.clone())
                .unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context).unwrap();

            // Add our people
            for _ in 1..num_people {
                let person_id = context.add_person(()).unwrap();
                context.add_itinerary(person_id, itinerary.clone()).unwrap();
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
            *num_people_isolating.borrow() as f64 / (num_people * num_sims) as f64;
        assert_almost_eq!(
            proportion_isolating,
            isolation_probability * (1.0 - proportion_asymptomatic),
            0.01
        );
    }

    #[test]
    fn test_isolation_guidance_input_validation() {
        // this test checks that the correct errors are raised when the input parameters
        // are not provided
        let mut context = Context::new();
        context.init_random(0);
        context
            .set_global_property_value(GlobalParams, Params::default())
            .unwrap();
        let e = super::init(&mut context).err();
        match e {
                Some(IxaError::IxaError(msg)) => {
                    assert_eq!(
                        msg,
                        "No facemask parameters provided. They are required for the intervention policy."
                    );
                }
                Some(ue) => panic!(
                    "Expected an error that initialization should fail due to unsupplied parameters. Instead got {:?}",
                    ue.to_string()
                ),
                None => panic!("Expected an error. Instead, policy initialized with no errors."),
            }

        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    facemask_parameters: Some(FacemaskParameters {
                        facemask_efficacy: 0.5,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        let e = super::init(&mut context).err();
        match e {
                Some(IxaError::IxaError(msg)) => {
                    assert_eq!(
                        msg,
                        "Policy enum does not match specified enum variant for updated guidance."
                    );
                }
                Some(ue) => panic!(
                    "Expected an error that initialization should fail due to unsupplied parameters. Instead got {:?}",
                    ue.to_string()
                ),
                None => panic!("Expected an error. Instead, policy initialized with no errors."),
            }
    }
}
