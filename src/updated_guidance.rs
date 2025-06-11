use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonId, PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    settings::{ContextSettingExt, Home, ItineraryModifiers},
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_derived_property!(PresentingWithSymptoms, bool, [Symptoms], |data| matches!(
    data,
    Some(
        SymptomValue::Category1
            | SymptomValue::Category2
            | SymptomValue::Category3
            | SymptomValue::Category4,
    )
));
define_rng!(PolicyRng);

trait ContextIsolationGuidanceInternalExt {
    fn modify_isolation_status(&mut self, person: PersonId, isolation_status: bool);
    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        isolation_probability: f64,
        isolation_delay_period: f64,
    );
}

impl ContextIsolationGuidanceInternalExt for Context {
    fn modify_isolation_status(&mut self, person: PersonId, isolation_status: bool) {
        if isolation_status {
            let _ =
                self.modify_itinerary(person, ItineraryModifiers::RestrictTo { setting: &Home });
        } else {
            let _ = self.remove_modified_itinerary(person);
        }
        self.set_person_property(person, IsolatingStatus, isolation_status);
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        isolation_probability: f64,
        isolation_delay_period: f64,
    ) {
        self.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                if event.current {
                    if context.sample_bool(PolicyRng, isolation_probability) {
                        context.add_plan(
                            context.get_current_time() + isolation_delay_period,
                            move |context| {
                                // Check that person still has symptoms after the delay
                                if context
                                    .get_person_property(event.person_id, Symptoms)
                                    .is_some()
                                {
                                    context.modify_isolation_status(event.person_id, true);
                                    trace!("Person {} is now isolating", event.person_id);
                                }
                            },
                        );
                    }
                } else if event.previous {
                    // individuals have recovered from symptoms
                    if context.get_person_property(event.person_id, IsolatingStatus) {
                        context.modify_isolation_status(event.person_id, false);
                        context.set_person_property(event.person_id, MaskingStatus, true);
                        trace!(
                            "Person {} is now masking and no longer isolating",
                            event.person_id
                        );
                        context.add_plan(
                            context.get_current_time() + post_isolation_duration,
                            move |context| {
                                context.set_person_property(event.person_id, MaskingStatus, false);
                                trace!("Person {} is now no longer masking", event.person_id);
                            },
                        );
                    }
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
    }

    if let Some(intervention_policy_parameters) = intervention_policy_parameters {
        context.setup_isolation_guidance_event_sequence(
            intervention_policy_parameters.post_isolation_duration,
            intervention_policy_parameters.isolation_probability,
            intervention_policy_parameters.isolation_delay_period,
        );
    } else {
        trace!("No isolation policy parameters provided. Skipping isolation guidance setup.");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use super::{init as policy_init, PresentingWithSymptoms};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            FacemaskParameters, GlobalParams, InterventionPolicyParameters, ProgressionLibraryType,
            RateFnType,
        },
        population_loader::Alive,
        rate_fns::load_rate_fns,
        symptom_progression::init as symptom_init,
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
        isolation_probability: f64,
        isolation_delay_period: f64,
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
            settings_properties: HashMap::new(),
            intervention_policy_parameters: Some(InterventionPolicyParameters {
                post_isolation_duration,
                isolation_probability,
                isolation_delay_period,
            }),
            facemask_parameters: Some(FacemaskParameters { facemask_efficacy }),
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

        let mut context = setup_context(
            post_isolation_duration,
            isolation_probability,
            isolation_delay_period,
            facemask_efficacy,
        );
        let p1 = context.add_person(()).unwrap();
        symptom_init(&mut context).unwrap();
        policy_init(&mut context);

        let start_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let end_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let start_time_isolation = Rc::new(RefCell::new(0.0f64));
        let end_time_isolation = Rc::new(RefCell::new(0.0f64));
        let start_time_mask = Rc::new(RefCell::new(0.0f64));
        let end_time_mask = Rc::new(RefCell::new(0.0f64));

        let start_time_symptoms_clone1 = start_time_symptoms.clone();
        let end_time_symptoms_clone1 = end_time_symptoms.clone();
        let start_time_isolation_clone1 = start_time_isolation.clone();
        let end_time_isolation_clone1 = end_time_isolation.clone();
        let start_time_mask_clone1 = start_time_mask.clone();
        let end_time_mask_clone1 = end_time_mask.clone();

        context.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                if event.current {
                    *start_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                } else if event.previous {
                    *end_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                if event.current {
                    *start_time_isolation_clone1.borrow_mut() = context.get_current_time();
                } else {
                    *end_time_isolation_clone1.borrow_mut() = context.get_current_time();
                }
            },
        );

        context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
            move |context, event| {
                if event.current {
                    *start_time_mask_clone1.borrow_mut() = context.get_current_time();
                } else {
                    *end_time_mask_clone1.borrow_mut() = context.get_current_time();
                }
            },
        );
        context.infect_person(p1, None, None, None);
        context.execute();

        assert_almost_eq!(
            *start_time_symptoms.borrow() + isolation_delay_period,
            *start_time_isolation.borrow(),
            0.0
        );
        assert_almost_eq!(
            *end_time_isolation.borrow(),
            *end_time_symptoms.borrow(),
            0.0
        );
        assert_almost_eq!(*start_time_mask.borrow(), *end_time_isolation.borrow(), 0.0);
        assert_almost_eq!(
            *end_time_mask.borrow(),
            *start_time_mask.borrow() + post_isolation_duration,
            0.0
        );
    }
    #[test]
    fn test_isolation_guidance_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the isolation probability parameter.
        // Note this requires an uptake delay period of 0.

        let post_isolation_duration = 5.0;
        let isolation_probability = 1.0;
        let isolation_delay_period = 0.0;
        let facemask_efficacy = 0.5;

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
        assert_almost_eq!(proportion_isolating, isolation_probability, 0.01);
    }
}
