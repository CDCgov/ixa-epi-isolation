use std::f64;

use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, InterventionPolicyParameters, Params},
    settings::{ContextSettingExt, Home, ItineraryModifiers},
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_person_property_with_default!(LastTestResult, bool, false);
define_person_property_with_default!(SymptomStartTime, f64, 0.0);

define_derived_property!(PresentingWithSymptoms, bool, [Symptoms], |symptom_value| {
    match symptom_value {
        Some(SymptomValue::Presymptomatic) | None => false,
        Some(_) => true,
    }
});
define_rng!(PolicyRng);

trait ContextIsolationGuidanceInternalExt {
    fn modify_isolation_status(
        &mut self,
        person: PersonId,
        isolation_status: bool,
    ) -> Result<(), IxaError>;
    fn isolation(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    );
    fn post_isolation(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    );
    fn administer_test(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    );
    fn handle_symptom_resolution(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    );
    fn setup_isolation_guidance_event_sequence(
        &mut self,
        intervention_policy_parameters: InterventionPolicyParameters,
    );
}

impl ContextIsolationGuidanceInternalExt for Context {
    fn modify_isolation_status(
        &mut self,
        person: PersonId,
        isolation_status: bool,
    ) -> Result<(), IxaError> {
        self.set_person_property(person, IsolatingStatus, isolation_status);
        if isolation_status {
            self.modify_itinerary(person, ItineraryModifiers::RestrictTo { setting: &Home })?;
        } else {
            self.remove_modified_itinerary(person)?;
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
                    context.modify_isolation_status(person_id, true).unwrap();
                    trace!("Person {person_id} is now isolating");
                },
            );
            if !self.get_person_property(person_id, LastTestResult) {
                self.add_plan(
                    self.get_current_time()
                        + intervention_policy_parameters.negative_test_isolation_duration,
                    move |context| {
                        if context.get_person_property(person_id, PresentingWithSymptoms) {
                            context.administer_test(person_id, intervention_policy_parameters);
                            trace!("Person {person_id} was tested");
                        }
                    },
                );
            }
        }
    }

    fn post_isolation(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        let proposed_masking_end_time = self.get_person_property(person_id, SymptomStartTime)
            + intervention_policy_parameters.duration_from_symptom_onset;
        if proposed_masking_end_time > self.get_current_time() {
            self.set_person_property(person_id, MaskingStatus, true);
            trace!("Person {person_id} is now masking and no longer isolating");

            self.add_plan(proposed_masking_end_time, move |context| {
                context.set_person_property(person_id, MaskingStatus, false);
                trace!("Person {person_id} is now no longer masking");
            });
        }
    }

    fn administer_test(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if self.sample_bool(PolicyRng, intervention_policy_parameters.test_sensitivity) {
            trace!("Person {person_id} tested positive.");
            self.set_person_property(person_id, LastTestResult, true);
        } else {
            trace!("Person {person_id} tested negative.");
            self.set_person_property(person_id, LastTestResult, false);
        }
    }

    fn handle_symptom_resolution(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if self.get_person_property(person_id, IsolatingStatus) {
            let mut remaining_isolation_duration = -1.0;
            let symptoms = self.get_person_property(person_id, Symptoms);
            let current_duration_isolation =
                self.get_current_time() - self.get_person_property(person_id, SymptomStartTime);
            if self.get_person_property(person_id, LastTestResult) {
                if symptoms == Some(SymptomValue::Category1)
                    || symptoms == Some(SymptomValue::Category2)
                {
                    remaining_isolation_duration = f64::max(
                        intervention_policy_parameters.mild_symptom_isolation_duration
                            - current_duration_isolation,
                        0.0,
                    );
                    self.add_plan(
                        self.get_current_time() + remaining_isolation_duration,
                        move |context| {
                            context.modify_isolation_status(person_id, false).unwrap();
                            trace!("Person {person_id} is now no longer isolating");
                            context.post_isolation(person_id, intervention_policy_parameters);
                        },
                    );
                } else if symptoms == Some(SymptomValue::Category3)
                    || symptoms == Some(SymptomValue::Category4)
                {
                    remaining_isolation_duration = f64::max(
                        intervention_policy_parameters.moderate_symptom_isolation_duration
                            - current_duration_isolation,
                        0.0,
                    );
                    self.add_plan(
                        self.get_current_time() + remaining_isolation_duration,
                        move |context| {
                            context.modify_isolation_status(person_id, false).unwrap();
                            trace!("Person {person_id} is now no longer isolating");
                        },
                    );
                }
            }
        }
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<PresentingWithSymptoms>| {
                if event.current {
                    context.administer_test(event.person_id, intervention_policy_parameters);
                    context.isolation(event.person_id, intervention_policy_parameters);
                } else if event.previous {
                    context
                        .handle_symptom_resolution(event.person_id, intervention_policy_parameters);
                }
            },
        );
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<LastTestResult>| {
                if event.current {
                    context.isolation(event.person_id, intervention_policy_parameters);
                } else {
                    trace!("Person {} tested negative, no isolation.", event.person_id);
                }
            },
        );
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
                &[(true, 1.0 - facemask_parameters.facemask_efficacy)],
            )
            .unwrap();
    } else {
        trace!("No facemask parameters provided. Facemasks will have no impact.");
    }

    if let Some(intervention_policy_parameters) = intervention_policy_parameters {
        context.setup_isolation_guidance_event_sequence(intervention_policy_parameters);
    } else {
        trace!("No isolation policy parameters provided. Skipping isolation guidance setup.");
    }
}

#[cfg(test)]
mod test {
    use super::PresentingWithSymptoms;
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            CoreSettingsTypes, FacemaskParameters, GlobalParams, InterventionPolicyParameters,
            ItinerarySpecificationType, ProgressionLibraryType, RateFnType,
        },
        population_loader::Alive,
        previous_guidance::test,
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
        ContextRandomExt, PersonPropertyChangeEvent,
    };

    use super::{IsolatingStatus, MaskingStatus};

    use statrs::assert_almost_eq;

    #[allow(clippy::too_many_arguments)]
    fn setup_context(
        duration_from_symptom_onset: f64,
        mild_symptom_isolation_duration: f64,
        moderate_symptom_isolation_duration: f64,
        negative_test_isolation_duration: f64,
        isolation_probability: f64,
        isolation_delay_period: f64,
        test_sensitivity: f64,
        facemask_efficacy: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
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
            intervention_policy_parameters: Some(InterventionPolicyParameters {
                duration_from_symptom_onset,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                negative_test_isolation_duration,
                isolation_probability,
                isolation_delay_period,
                test_sensitivity,
            }),
            facemask_parameters: Some(FacemaskParameters { facemask_efficacy }),
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
        let duration_from_symptom_onset = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let negative_test_isolation_duration = 2.0;
        let isolation_probability = 1.0;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 0.9;
        let facemask_efficacy = 0.5;

        let mut context = setup_context(
            duration_from_symptom_onset,
            mild_symptom_isolation_duration,
            moderate_symptom_isolation_duration,
            negative_test_isolation_duration,
            isolation_probability,
            isolation_delay_period,
            test_sensitivity,
            facemask_efficacy,
        );
        let p1 = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0),
        ];
        context.add_itinerary(p1, itinerary).unwrap();
        crate::symptom_progression::init(&mut context).unwrap();
        super::init(&mut context);

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
                            + mild_symptom_isolation_duration,
                        context.get_current_time(),
                        0.0
                    );
                }
            },
        );
        context.infect_person(p1, None, None, None);
        context.execute();
    }

    #[test]
    fn test_isolation_guidance_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the isolation probability parameter.
        // Note this requires an isolation delay period of 0.

        let duration_from_symptom_onset = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let negative_test_isolation_duration = 2.0;
        let isolation_probability = 1.0;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 0.9;
        let facemask_efficacy = 0.5;

        let num_people_isolating = Rc::new(RefCell::new(0usize));

        let num_people = 1000;
        let num_sims = 100;
        for seed in 0..num_sims {
            let num_people_isolating_clone = Rc::clone(&num_people_isolating);
            let mut context = setup_context(
                duration_from_symptom_onset,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                negative_test_isolation_duration,
                isolation_probability,
                isolation_delay_period,
                test_sensitivity,
                facemask_efficacy,
            );
            context.init_random(seed);
            let first_person = context.add_person(()).unwrap();
            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0),
                ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0),
            ];
            context
                .add_itinerary(first_person, itinerary.clone())
                .unwrap();
            crate::symptom_progression::init(&mut context).unwrap();
            super::init(&mut context);

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
        assert_almost_eq!(proportion_isolating, isolation_probability, 0.01);
    }
}
