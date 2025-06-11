use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, InterventionPolicyParameters, Params},
    settings::{
        CensusTract, ContextSettingExt, Home, ItineraryEntry, ItineraryModifiers, SettingId,
    },
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
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
    fn modify_masking_status(
        &mut self,
        person: PersonId,
        masking_status: bool,
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

    fn modify_masking_status(
        &mut self,
        person: PersonId,
        masking_status: bool,
    ) -> Result<(), IxaError> {
        self.set_person_property(person, MaskingStatus, masking_status);
        if masking_status {
            let isolation_itinerary = vec![
                ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
                ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
            ];
            self.modify_itinerary(
                person,
                ItineraryModifiers::ReplaceWith {
                    itinerary: isolation_itinerary,
                },
            )?;
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
        if self.sample_bool(PolicyRng, intervention_policy_parameters.uptake_probability) {
            let t = self.get_current_time();
            self.add_plan(
                t + intervention_policy_parameters.uptake_delay_period,
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
            self.modify_masking_status(person_id, true).unwrap();
            trace!("Person {person_id} is now masking and no longer isolating");
            let t = self.get_current_time();
            self.add_plan(
                t + intervention_policy_parameters.post_isolation_duration,
                move |context| {
                    context.modify_masking_status(person_id, false).unwrap();
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
                match (event.current, event.previous) {
                    // individuals transition from not presenting with symptoms to presenting with symptoms
                    (true, false) => {
                        context.isolation(event.person_id, intervention_policy_parameters);
                    }
                    //individuals transition from presenting with symptoms to not presenting with symptoms
                    (false, true) => {
                        context.post_isolation(event.person_id, intervention_policy_parameters);
                    }
                    _ => (),
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

    fn setup_context(
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
        facemask_efficacy: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
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
                post_isolation_duration,
                uptake_probability,
                uptake_delay_period,
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
        // this test checks that times at which and individual starts and
        // stops the isolating and masking is correct relative to the symptom onset
        // and the intervention policy parameters. We expect an individual to begin isolating
        // an uptake_delay_period after they start presenting with symptoms, and to stop isolating
        // when symptoms end. We expect an individual to start masking when symptoms end and mask
        // for post_isolation_duration days.
        let post_isolation_duration = 5.0;
        let uptake_probability = 1.0;
        let uptake_delay_period = 1.0;
        let facemask_efficacy = 0.5;

        let mut context = setup_context(
            post_isolation_duration,
            uptake_probability,
            uptake_delay_period,
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
            move |context, event| match (event.current, event.previous) {
                (true, false) => {
                    context.set_person_property(
                        event.person_id,
                        SymptomStartTime,
                        context.get_current_time(),
                    );
                }
                (false, true) => {
                    context.set_person_property(
                        event.person_id,
                        SymptomEndTime,
                        context.get_current_time(),
                    );
                }
                _ => (),
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                if event.current {
                    assert_almost_eq!(
                        context.get_person_property(event.person_id, SymptomStartTime)
                            + uptake_delay_period,
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
                }
            },
        );
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
        assert_almost_eq!(proportion_isolating, uptake_probability, 0.01);
    }
}
