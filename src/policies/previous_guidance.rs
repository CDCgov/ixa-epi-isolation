use std::f64;

use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent, PluginContext,
};

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    policies::Policies,
    settings::{ContextSettingExt, Home, ItineraryModifiers},
    symptom_progression::{PresentingWithSymptoms, SymptomRecord},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_person_property_with_default!(LastTestResult, Option<bool>, None);

define_rng!(PreviousPolicyRng);

#[derive(Debug, Clone, Copy)]
struct InterventionPolicyParameters {
    overall_policy_duration: f64,
    mild_symptom_isolation_duration: f64,
    moderate_symptom_isolation_duration: f64,
    delay_to_retest: f64,
    policy_adherence: f64,
    isolation_delay_period: f64,
    test_sensitivity: f64,
}

trait ContextIsolationGuidanceInternalExt:
    PluginContext + ContextRandomExt + ContextPeopleExt + ContextSettingExt
{
    fn modify_itinerary_and_isolation_status(
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

    fn make_isolation_plan(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        self.add_plan(
            self.get_current_time() + intervention_policy_parameters.isolation_delay_period,
            move |context| {
                if context.get_person_property(person_id, PresentingWithSymptoms) {
                    context.administer_test_and_schedule_any_followup(
                        person_id,
                        intervention_policy_parameters,
                    );
                    context
                        .modify_itinerary_and_isolation_status(person_id, true)
                        .unwrap();
                    trace!("Person {person_id} is now isolating");
                }
            },
        );
    }

    fn make_post_isolation_masking_plan(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if let Some(symptom_record) = self.get_person_property(person_id, SymptomRecord) {
            let proposed_masking_end_time = symptom_record.symptom_start
                + intervention_policy_parameters.overall_policy_duration;
            if proposed_masking_end_time > self.get_current_time() {
                self.set_person_property(person_id, MaskingStatus, true);
                trace!("Person {person_id} is now masking");

                self.add_plan(proposed_masking_end_time, move |context| {
                    context.set_person_property(person_id, MaskingStatus, false);
                    trace!("Person {person_id} is now no longer masking");
                });
            }
        }
    }

    fn administer_test_and_schedule_any_followup(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        let last_test_result = self.get_person_property(person_id, LastTestResult);
        let mut retest = false;

        if self.get_person_property(person_id, InfectionStatus) == InfectionStatusValue::Infectious
        {
            if self.sample_bool(
                PreviousPolicyRng,
                intervention_policy_parameters.test_sensitivity,
            ) {
                // A known positive doesn't require a retest, keeping default `false`
                self.set_person_property(person_id, LastTestResult, Some(true));
            } else {
                if last_test_result.is_none() {
                    retest = true;
                }
                self.set_person_property(person_id, LastTestResult, Some(false));
            }
        } else {
            if last_test_result.is_none() {
                retest = true;
            }
            self.set_person_property(person_id, LastTestResult, Some(false));
        }
        trace!("Person {person_id} was tested");

        if retest {
            self.schedule_retest(person_id, intervention_policy_parameters);
        }
    }

    fn schedule_retest(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        self.add_plan(
            self.get_current_time() + intervention_policy_parameters.delay_to_retest,
            move |context| {
                if context.get_person_property(person_id, PresentingWithSymptoms) {
                    context.administer_test_and_schedule_any_followup(
                        person_id,
                        intervention_policy_parameters,
                    );
                }
            },
        );
    }

    fn handle_symptom_resolution(
        &mut self,
        person_id: PersonId,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        if self
            .get_person_property(person_id, LastTestResult)
            .unwrap_or(false)
        {
            if let Some(symptom_record) = self.get_person_property(person_id, SymptomRecord) {
                // even if there is a delay to start isolation, people's isolation time is counted
                // from their symptom start time
                let minimum_isolation_time = if symptom_record.severe {
                    intervention_policy_parameters.moderate_symptom_isolation_duration
                        + symptom_record.symptom_start
                } else {
                    intervention_policy_parameters.mild_symptom_isolation_duration
                        + symptom_record.symptom_start
                };
                let isolation_end = f64::max(minimum_isolation_time, self.get_current_time());
                self.add_plan(isolation_end, move |context| {
                    context
                        .modify_itinerary_and_isolation_status(person_id, false)
                        .unwrap();
                    trace!("Person {person_id} is now no longer isolating");
                    if !symptom_record.severe {
                        context.make_post_isolation_masking_plan(
                            person_id,
                            intervention_policy_parameters,
                        );
                    }
                });
            }
        } else {
            self.modify_itinerary_and_isolation_status(person_id, false)
                .unwrap();
        }
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        intervention_policy_parameters: InterventionPolicyParameters,
    ) {
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<PresentingWithSymptoms>| {
                if event.current {
                    if context.sample_bool(
                        PreviousPolicyRng,
                        intervention_policy_parameters.policy_adherence,
                    ) {
                        context
                            .make_isolation_plan(event.person_id, intervention_policy_parameters);
                    }
                } else if event.previous
                    && context.get_person_property(event.person_id, IsolatingStatus)
                {
                    context
                        .handle_symptom_resolution(event.person_id, intervention_policy_parameters);
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
        Some(Policies::PreviousIsolationGuidance {
            overall_policy_duration,
            mild_symptom_isolation_duration,
            moderate_symptom_isolation_duration,
            delay_to_retest,
            policy_adherence,
            isolation_delay_period,
            test_sensitivity,
        }) => {
            let intervention_policy_parameters = InterventionPolicyParameters {
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
            };
            context.setup_isolation_guidance_event_sequence(intervention_policy_parameters);
        }
        _ => {
            return Err(IxaError::IxaError(
                "Policy enum does not match specified enum variant for previous guidance."
                    .to_string(),
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            CoreSettingsTypes, FacemaskParameters, GlobalParams, ItinerarySpecificationType,
            ProgressionLibraryType, RateFnType,
        },
        policies::{previous_guidance::LastTestResult, Policies},
        population_loader::Alive,
        rate_fns::load_rate_fns,
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, SettingId, SettingProperties,
            Workplace,
        },
        symptom_progression::{PresentingWithSymptoms, SymptomRecord},
        Params,
    };
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use ixa::{
        define_person_property_with_default, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, HashMap, IxaError, PersonPropertyChangeEvent,
    };

    use super::{IsolatingStatus, MaskingStatus};

    use statrs::assert_almost_eq;
    #[allow(clippy::too_many_arguments)]
    fn setup_context(
        overall_policy_duration: f64,
        mild_symptom_isolation_duration: f64,
        moderate_symptom_isolation_duration: f64,
        delay_to_retest: f64,
        policy_adherence: f64,
        isolation_delay_period: f64,
        test_sensitivity: f64,
        facemask_efficacy: f64,
        proportion_asymptomatic: f64,
        seed: u64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
            proportion_asymptomatic,
            max_time: 100.0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_empirical_rate_fns.csv"),
                scale: 0.05,
            },
            symptom_progression_library: Some(ProgressionLibraryType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_symptom_parameters.csv"),
            }),
            settings_properties: HashMap::from_iter(
                [
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
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            guidance_policy: Some(Policies::PreviousIsolationGuidance {
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
            }),
            facemask_parameters: Some(FacemaskParameters { facemask_efficacy }),
            ..Default::default()
        };
        context.init_random(seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        crate::settings::init(&mut context);
        context
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_isolation_guidance_event_sequence_positive_test() {
        // 1. Create a new person and set test sensitivity = 1
        // 2. Keep track of the time of symptom onset and end time
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that end of isolation is at the max(end of symptoms, end of isolation duration)
        //   and correctly account for symptom severity
        // 5. Assert that start of facemask is end of isolation and occurs for correct symptom severity
        // 6. Assert that end of facemask is end of isolation + post isolation days
        // 7. Assert that either a moderate or mild outcome occurred but not both.

        let overall_policy_duration = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let delay_to_retest = 2.0;
        let policy_adherence = 1.0;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 1.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.0;
        let num_sims = 100;
        for seed in 0..num_sims {
            let mut context = setup_context(
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
                facemask_efficacy,
                proportion_asymptomatic,
                seed,
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

            let mild_policy_ran_flag = Rc::new(RefCell::new(false));
            let mild_policy_ran_flag_clone = Rc::clone(&mild_policy_ran_flag);
            let mild_no_masking_flag = Rc::new(RefCell::new(false));
            let mild_no_masking_flag_clone = Rc::clone(&mild_no_masking_flag);
            let moderate_policy_ran_flag = Rc::new(RefCell::new(false));
            let moderate_policy_ran_flag_clone = Rc::clone(&moderate_policy_ran_flag);

            define_person_property_with_default!(IsolationEndTime, f64, 0.0);

            context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
                move |context, event| {
                    if event.current {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                                + isolation_delay_period,
                            context.get_current_time(),
                            0.000_001
                        );
                    } else {
                        let severe_symptoms = context
                            .get_person_property(event.person_id, SymptomRecord)
                            .unwrap()
                            .severe;
                        context.set_person_property(
                            event.person_id,
                            IsolationEndTime,
                            context.get_current_time(),
                        );
                        if severe_symptoms {
                            assert_almost_eq!(
                                f64::max(
                                    context
                                        .get_person_property(event.person_id, SymptomRecord)
                                        .unwrap()
                                        .symptom_end
                                        .unwrap(),
                                    context
                                        .get_person_property(event.person_id, SymptomRecord)
                                        .unwrap()
                                        .symptom_start
                                        + moderate_symptom_isolation_duration
                                ),
                                context.get_current_time(),
                                0.000_001
                            );
                            *moderate_policy_ran_flag_clone.borrow_mut() = true;
                        } else {
                            assert_almost_eq!(
                                f64::max(
                                    context
                                        .get_person_property(event.person_id, SymptomRecord)
                                        .unwrap()
                                        .symptom_end
                                        .unwrap(),
                                    context
                                        .get_person_property(event.person_id, SymptomRecord)
                                        .unwrap()
                                        .symptom_start
                                        + mild_symptom_isolation_duration
                                ),
                                context.get_current_time(),
                                0.000_001
                            );
                            if context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_end
                                .unwrap()
                                > context
                                    .get_person_property(event.person_id, SymptomRecord)
                                    .unwrap()
                                    .symptom_start
                                    + overall_policy_duration
                            {
                                *mild_no_masking_flag_clone.borrow_mut() = true;
                            }
                        }
                    }
                },
            );

            context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
                move |context, event| {
                    if event.current {
                        //assert size of population masking equals the size of the population masking
                        assert_almost_eq!(
                            context.get_person_property(event.person_id, IsolationEndTime),
                            context.get_current_time(),
                            0.000_001
                        );
                        let severity = context
                            .get_person_property(event.person_id, SymptomRecord)
                            .unwrap()
                            .severe;
                        assert!(!severity);
                    } else {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                                + overall_policy_duration,
                            context.get_current_time(),
                            0.000_001
                        );
                        *mild_policy_ran_flag_clone.borrow_mut() = true;
                    }
                },
            );
            context.infect_person(p1, None, None, None);
            context.execute();

            let long_delay_flag = context
                .get_person_property(p1, SymptomRecord)
                .unwrap()
                .symptom_end
                .unwrap()
                - context
                    .get_person_property(p1, SymptomRecord)
                    .unwrap()
                    .symptom_start
                <= isolation_delay_period;

            assert!(
                *mild_policy_ran_flag.borrow()
                    ^ *moderate_policy_ran_flag.borrow()
                    ^ *mild_no_masking_flag.borrow()
                    ^ long_delay_flag,
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_isolation_guidance_event_sequence_negative_tests() {
        // 1. Create a new person and set test sensitivity = 0
        // 2. Keep track of the time of symptom onset and end time
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that end of isolation is at the end of symptoms for all severities
        // 5. Assert that the correct number of tests were administered
        // 6. Assert that the policy was completed

        let overall_policy_duration = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let delay_to_retest = 2.0;
        let policy_adherence = 1.0;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 0.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.0;
        let num_sims = 100;
        for seed in 0..num_sims {
            let mut context = setup_context(
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
                facemask_efficacy,
                proportion_asymptomatic,
                seed,
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

            define_person_property_with_default!(NumberOfTests, usize, 0);

            context.subscribe_to_event::<PersonPropertyChangeEvent<LastTestResult>>(
                move |context, event| {
                    context.set_person_property(
                        event.person_id,
                        NumberOfTests,
                        context.get_person_property(event.person_id, NumberOfTests) + 1,
                    );
                },
            );

            context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
                move |context, event| {
                    if event.current {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                                + isolation_delay_period,
                            context.get_current_time(),
                            0.000_001
                        );
                    } else {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_end
                                .unwrap(),
                            context.get_current_time(),
                            0.000_001
                        );
                        if context
                            .get_person_property(event.person_id, SymptomRecord)
                            .unwrap()
                            .symptom_end
                            .unwrap()
                            - context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                            < delay_to_retest + isolation_delay_period
                        {
                            assert_eq!(
                                1,
                                context.get_person_property(event.person_id, NumberOfTests)
                            );
                        } else {
                            assert_eq!(
                                2,
                                context.get_person_property(event.person_id, NumberOfTests)
                            );
                        }
                        *policy_ran_flag_clone.borrow_mut() = true;
                    }
                },
            );
            context.infect_person(p1, None, None, None);
            context.execute();

            let long_delay_flag = context
                .get_person_property(p1, SymptomRecord)
                .unwrap()
                .symptom_end
                .unwrap()
                - context
                    .get_person_property(p1, SymptomRecord)
                    .unwrap()
                    .symptom_start
                <= isolation_delay_period;

            if long_delay_flag {
                assert_eq!(0, context.get_person_property(p1, NumberOfTests));
            }

            assert!(*policy_ran_flag.borrow() ^ long_delay_flag);
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_isolation_guidance_event_sequence_test_negative_then_positive() {
        // 1. Create a new person and set test sensitivity = 0.5
        // 2. Keep track of the time of symptom onset and end time
        //  - Filter out indidivuals that have 1 positive test or two negative tests
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that end of isolation is at the max(end of symptoms, end of isolation duration)
        //   and correctly account for symptom severity
        // 5. Assert that start of facemask is end of isolation and occurs for correct symptom severity
        // 6. Assert that end of facemask is end of isolation + post isolation days
        // 7. Assert that the policy ran or the simulation shutdown

        let overall_policy_duration = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let delay_to_retest = 2.0;
        let policy_adherence = 1.0;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 0.5;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.0;
        let num_sims = 500;
        for seed in 0..num_sims {
            let mut context = setup_context(
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
                facemask_efficacy,
                proportion_asymptomatic,
                seed,
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

            let mild_policy_ran_flag = Rc::new(RefCell::new(false));
            let mild_policy_ran_flag_clone = Rc::clone(&mild_policy_ran_flag);
            let moderate_policy_ran_flag = Rc::new(RefCell::new(false));
            let moderate_policy_ran_flag_clone = Rc::clone(&moderate_policy_ran_flag);
            let mild_policy_no_masking_flag = Rc::new(RefCell::new(false));
            let mild_policy_no_masking_flag_clone = Rc::clone(&mild_policy_no_masking_flag);
            let early_exit_flag = Rc::new(RefCell::new(false));
            let early_exit_flag_clone = Rc::clone(&early_exit_flag);
            let shutdown_flag = Rc::new(RefCell::new(false));
            let shutdown_flag_clone = Rc::clone(&shutdown_flag);

            define_person_property_with_default!(NumberOfTests, usize, 0);
            define_person_property_with_default!(IsolationEndTime, f64, 0.0);

            context.subscribe_to_event::<PersonPropertyChangeEvent<LastTestResult>>(
                move |context, event| {
                    if event.current.unwrap_or(false)
                        && context.get_person_property(event.person_id, NumberOfTests) == 0
                    {
                        context.shutdown();
                        *shutdown_flag_clone.borrow_mut() = true;
                    }
                    if !event.current.unwrap_or(false)
                        && context.get_person_property(event.person_id, NumberOfTests) == 1
                    {
                        context.shutdown();
                        *shutdown_flag_clone.borrow_mut() = true;
                    }
                    context.set_person_property(
                        event.person_id,
                        NumberOfTests,
                        context.get_person_property(event.person_id, NumberOfTests) + 1,
                    );
                },
            );

            context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
                move |context, event| {
                    if event.current {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                                + isolation_delay_period,
                            context.get_current_time(),
                            0.000_001
                        );
                    } else {
                        let severe_symptoms = context
                            .get_person_property(event.person_id, SymptomRecord)
                            .unwrap()
                            .severe;
                        context.set_person_property(
                            event.person_id,
                            IsolationEndTime,
                            context.get_current_time(),
                        );
                        if context.get_person_property(event.person_id, NumberOfTests) == 2 {
                            if severe_symptoms {
                                assert_almost_eq!(
                                    f64::max(
                                        context
                                            .get_person_property(event.person_id, SymptomRecord)
                                            .unwrap()
                                            .symptom_end
                                            .unwrap(),
                                        context
                                            .get_person_property(event.person_id, SymptomRecord)
                                            .unwrap()
                                            .symptom_start
                                            + moderate_symptom_isolation_duration
                                    ),
                                    context.get_current_time(),
                                    0.000_001
                                );
                                *moderate_policy_ran_flag_clone.borrow_mut() = true;
                            } else {
                                assert_almost_eq!(
                                    f64::max(
                                        context
                                            .get_person_property(event.person_id, SymptomRecord)
                                            .unwrap()
                                            .symptom_end
                                            .unwrap(),
                                        context
                                            .get_person_property(event.person_id, SymptomRecord)
                                            .unwrap()
                                            .symptom_start
                                            + mild_symptom_isolation_duration
                                    ),
                                    context.get_current_time(),
                                    0.000_001
                                );
                                if context
                                    .get_person_property(event.person_id, SymptomRecord)
                                    .unwrap()
                                    .symptom_end
                                    .unwrap()
                                    > context
                                        .get_person_property(event.person_id, SymptomRecord)
                                        .unwrap()
                                        .symptom_start
                                        + overall_policy_duration
                                {
                                    *mild_policy_no_masking_flag_clone.borrow_mut() = true;
                                }
                            }
                        } else {
                            assert_almost_eq!(
                                context
                                    .get_person_property(event.person_id, SymptomRecord)
                                    .unwrap()
                                    .symptom_end
                                    .unwrap(),
                                context.get_current_time(),
                                0.000_001
                            );
                            *early_exit_flag_clone.borrow_mut() = true;
                        }
                    }
                },
            );

            context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
                move |context, event| {
                    if event.current {
                        assert_almost_eq!(
                            context.get_person_property(event.person_id, IsolationEndTime),
                            context.get_current_time(),
                            0.000_001
                        );
                    } else {
                        assert_almost_eq!(
                            context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start
                                + overall_policy_duration,
                            context.get_current_time(),
                            0.000_001
                        );
                        *mild_policy_ran_flag_clone.borrow_mut() = true;
                    }
                },
            );

            context.infect_person(p1, None, None, None);
            context.execute();
            let mut long_delay_flag = false;
            if let Some(symptom_duration) = context
                .get_person_property(p1, SymptomRecord)
                .unwrap()
                .symptom_end
            {
                long_delay_flag = symptom_duration
                    - context
                        .get_person_property(p1, SymptomRecord)
                        .unwrap()
                        .symptom_start
                    <= isolation_delay_period;
            }

            assert!(
                *mild_policy_ran_flag.borrow()
                    ^ *moderate_policy_ran_flag.borrow()
                    ^ *early_exit_flag.borrow()
                    ^ *mild_policy_no_masking_flag.borrow()
                    ^ (*shutdown_flag.borrow() || long_delay_flag),
            );
        }
    }

    #[test]
    fn test_isolation_guidance_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the policy adherence, proportion of asymptomatic,
        // and proportion of indidividuals whose symptomatic period is shorter than the isolation_delay_period.
        // The last component is calculated from the simulation.

        let overall_policy_duration = 10.0;
        let mild_symptom_isolation_duration = 5.0;
        let moderate_symptom_isolation_duration = 10.0;
        let delay_to_retest = 2.0;
        let policy_adherence = 0.75;
        let isolation_delay_period = 1.0;
        let test_sensitivity = 1.0;
        let facemask_efficacy = 0.5;
        let proportion_asymptomatic = 0.2;

        let num_people_policy = Rc::new(RefCell::new(0usize));
        let num_people_short_symptoms = Rc::new(RefCell::new(0usize));
        let num_people = 1000;
        let num_sims = 100;
        for seed in 0..num_sims {
            let num_people_policy_clone = Rc::clone(&num_people_policy);
            let num_people_short_symptoms_clone = Rc::clone(&num_people_short_symptoms);
            let mut context = setup_context(
                overall_policy_duration,
                mild_symptom_isolation_duration,
                moderate_symptom_isolation_duration,
                delay_to_retest,
                policy_adherence,
                isolation_delay_period,
                test_sensitivity,
                facemask_efficacy,
                proportion_asymptomatic,
                seed,
            );
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

            context.subscribe_to_event::<PersonPropertyChangeEvent<LastTestResult>>(
                move |_context, event| {
                    if event.current.unwrap_or(false) {
                        *num_people_policy_clone.borrow_mut() += 1;
                    }
                },
            );
            context.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
                move |context, event| {
                    if event.previous {
                        let duration = context.get_current_time()
                            - context
                                .get_person_property(event.person_id, SymptomRecord)
                                .unwrap()
                                .symptom_start;
                        if duration <= isolation_delay_period {
                            *num_people_short_symptoms_clone.borrow_mut() += 1;
                        }
                    }
                },
            );

            context.execute();
        }

        #[allow(clippy::cast_precision_loss)]
        let proportion_policy = *num_people_policy.borrow() as f64 / (num_people * num_sims) as f64;
        #[allow(clippy::cast_precision_loss)]
        let proportion_short_symptoms = *num_people_short_symptoms.borrow() as f64
            / ((num_sims as f64 * num_people as f64) * (1.0 - proportion_asymptomatic));

        assert_almost_eq!(
            proportion_policy,
            policy_adherence * (1.0 - proportion_asymptomatic) * (1.0 - proportion_short_symptoms),
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
                        "Policy enum does not match specified enum variant for previous guidance."
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
