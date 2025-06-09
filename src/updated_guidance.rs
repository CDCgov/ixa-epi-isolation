use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_derived_property!(
    PresentingWithSymptoms,
    Option<bool>,
    [Symptoms],
    |data| match data {
        Some(SymptomValue::Presymptomatic) => Some(false),
        Some(
            SymptomValue::Category1
            | SymptomValue::Category2
            | SymptomValue::Category3
            | SymptomValue::Category4,
        ) => Some(true),
        None => None,
    }
);
define_rng!(PolicyRng);

trait ContextIsolationGuidanceInternalExt {
    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
    );
}

impl ContextIsolationGuidanceInternalExt for Context {
    fn setup_isolation_guidance_event_sequence(
        &mut self,
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
    ) {
        self.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                match event.current {
                    // individuals are not presenting with symptoms but are infected
                    Some(false) => (),
                    // individuals are presenting with symptoms
                    Some(true) => {
                        if context.sample_bool(PolicyRng, uptake_probability) {
                            let t = context.get_current_time();
                            context.add_plan(t + uptake_delay_period, move |context| {
                                if context
                                    .get_person_property(event.person_id, Symptoms)
                                    .is_some()
                                {
                                    context.set_person_property(
                                        event.person_id,
                                        IsolatingStatus,
                                        true,
                                    );
                                    trace!("Person {} is now isolating", event.person_id);
                                }
                            });
                        }
                    }
                    // individuals have recovered from symptoms
                    None => {
                        if context.get_person_property(event.person_id, IsolatingStatus) {
                            context.set_person_property(event.person_id, IsolatingStatus, false);
                            context.set_person_property(event.person_id, MaskingStatus, true);
                            trace!(
                                "Person {} is now wearing a mask and no longer isolating",
                                event.person_id
                            );
                            let t = context.get_current_time();
                            context.add_plan(t + post_isolation_duration, move |context| {
                                context.set_person_property(event.person_id, MaskingStatus, false);
                                trace!("Person {} is now not wearing a mask", event.person_id);
                            });
                        }
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
        isolation_parameters, // Not used in this function, but could be used for other purposes
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

    if let Some(isolation_parameters) = isolation_parameters {
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                IsolatingStatus,
                &[(true, 1.0 - isolation_parameters.isolation_efficacy)],
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
    use super::{init as policy_init, PresentingWithSymptoms};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{
            FacemaskParameters, GlobalParams, InterventionPolicyParameters, IsolationParameters,
            ProgressionLibraryType, RateFnType,
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

    use statrs::assert_almost_eq;

    use super::{IsolatingStatus, MaskingStatus};

    fn setup_context(
        post_isolation_duration: f64,
        uptake_probability: f64,
        uptake_delay_period: f64,
        facemask_efficacy: f64,
        isolation_efficacy: f64,
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
            facemask_parameters: Some(FacemaskParameters { facemask_efficacy }),
            isolation_parameters: Some(IsolationParameters { isolation_efficacy }),
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
        let facemask_efficacy = 0.5;
        let isolation_efficacy = 0.5;
        let mut context = setup_context(
            post_isolation_duration,
            uptake_probability,
            uptake_delay_period,
            facemask_efficacy,
            isolation_efficacy,
        );
        let p1 = context.add_person(()).unwrap();
        symptom_init(&mut context).unwrap();
        policy_init(&mut context);

        let start_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let end_time_symptoms = Rc::new(RefCell::new(0.0f64));

        let start_time_symptoms_clone1 = Rc::clone(&start_time_symptoms);
        let end_time_symptoms_clone1 = Rc::clone(&end_time_symptoms);
        context.subscribe_to_event::<PersonPropertyChangeEvent<PresentingWithSymptoms>>(
            move |context, event| {
                println!(
                    "symptom time: {}, event.current: {:?}",
                    context.get_current_time(),
                    event.current
                );
                match event.current {
                    Some(false) => (),
                    Some(true) => {
                        *start_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                    }
                    None => {
                        *end_time_symptoms_clone1.borrow_mut() = context.get_current_time();
                    }
                }
            },
        );

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
        context.infect_person(p1, None, None, None);
        context.execute();
    }
    #[test]
    fn test_isolation_guidance_uptake_probability() {
        // this test checks that the proportion of individuals that isolation is what we
        // expect. This proportion is determined by the uptake_probability parameter.
        let post_isolation_duration = 5.0;
        let uptake_probability = 1.0;
        let uptake_delay_period = 0.0;
        let facemask_efficacy = 0.5;
        let isolation_efficacy = 0.5;
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
                isolation_efficacy,
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
        // Check that the proportion of people who are symptomatic is close to the expected
        // proportion
        #[allow(clippy::cast_precision_loss)]
        let proportion_isolating =
            num_people_isolating.take() as f64 / (num_people * num_sims) as f64;
        assert_almost_eq!(proportion_isolating, uptake_probability, 0.01);
    }
}
