use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonPropertyChangeEvent,
};

use statrs::distribution::Uniform;

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::ContextParametersExt,
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

pub fn init(context: &mut Context) {
    let isolation_policy = context.get_params().isolation_policy_parameters.clone();

    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            MaskingStatus,
            &[(
                true,
                *isolation_policy
                    .get("facemask_transmission_modifier")
                    .unwrap(),
            )],
        )
        .unwrap();
    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            IsolatingStatus,
            &[(
                true,
                *isolation_policy
                    .get("isolation_transmission_modifier")
                    .unwrap(),
            )],
        )
        .unwrap();

    setup_isolation_guidance_event_sequence(
        context,
        *isolation_policy.get("post_isolation_duration").unwrap(),
        *isolation_policy.get("uptake_probability").unwrap(),
        *isolation_policy.get("maximum_uptake_delay").unwrap(),
    );
}

fn setup_isolation_guidance_event_sequence(
    context: &mut Context,
    post_isolation_duration: f64,
    uptake_probability: f64,
    maximum_uptake_delay: f64,
) {
    context.subscribe_to_event::<PersonPropertyChangeEvent<Symptomatic>>(move |context, event| {
        match event.current {
            Some(false) => (),
            Some(true) => {
                if context.sample_bool(PolicyRng, uptake_probability) {
                    let uniform = Uniform::new(0.0, maximum_uptake_delay).unwrap();
                    let delay_period = context.sample_distr(PolicyRng, uniform);
                    let t = context.get_current_time();
                    context.add_plan(t + delay_period, move |context| {
                        if context
                            .get_person_property(event.person_id, Symptoms)
                            .is_some()
                        {
                            context.set_person_property(event.person_id, IsolatingStatus, true);
                            trace!("Person {} is now isolating", event.person_id);
                        }
                    });
                }
            }
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
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use super::init as policy_init;
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, ProgressionLibraryType, RateFnType},
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

    fn setup_context(
        post_isolation_duration: f64,
        uptake_probability: f64,
        maximum_uptake_delay: f64,
        facemask_transmission_modifier: f64,
        isolation_transmission_modifier: f64,
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
            isolation_policy_parameters: [
                (
                    "post_isolation_duration".to_string(),
                    post_isolation_duration,
                ),
                ("uptake_probability".to_string(), uptake_probability),
                ("maximum_uptake_delay".to_string(), maximum_uptake_delay),
                (
                    "facemask_transmission_modifier".to_string(),
                    facemask_transmission_modifier,
                ),
                (
                    "isolation_transmission_modifier".to_string(),
                    isolation_transmission_modifier,
                ),
            ]
            .iter()
            .cloned()
            .collect(),
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
        let post_isolation_duration = 5.0;
        let uptake_probability = 1.0;
        let maximum_uptake_delay = 1.0;
        let facemask_transmission_modifier = 0.5;
        let isolation_transmission_modifier = 0.5;
        let mut context = setup_context(
            post_isolation_duration,
            uptake_probability,
            maximum_uptake_delay,
            facemask_transmission_modifier,
            isolation_transmission_modifier,
        );

        let start_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let end_time_symptoms = Rc::new(RefCell::new(0.0f64));
        let start_time_isolation = Rc::new(RefCell::new(0.0f64));
        let end_time_isolation = Rc::new(RefCell::new(0.0f64));
        let start_time_masking = Rc::new(RefCell::new(0.0f64));
        let end_time_masking = Rc::new(RefCell::new(0.0f64));

        let start_time_symptoms_clone = Rc::clone(&start_time_symptoms);
        let end_time_symptoms_clone = Rc::clone(&end_time_symptoms);
        let start_time_isolation_clone = Rc::clone(&start_time_isolation);
        let end_time_isolation_clone = Rc::clone(&end_time_isolation);
        let start_time_masking_clone = Rc::clone(&start_time_masking);
        let end_time_masking_clone = Rc::clone(&end_time_masking);
        context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(move |context, event| {
            match event.current {
                Some(SymptomValue::Presymptomatic) => (),
                Some(
                    SymptomValue::Category1
                    | SymptomValue::Category2
                    | SymptomValue::Category3
                    | SymptomValue::Category4,
                ) => {
                    *start_time_symptoms_clone.borrow_mut() = context.get_current_time();
                }
                None => {
                    *end_time_symptoms_clone.borrow_mut() = context.get_current_time();
                }
            }
        });
        context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
            move |context, event| {
                if event.current {
                    *start_time_isolation_clone.borrow_mut() = context.get_current_time();
                } else {
                    *end_time_isolation_clone.borrow_mut() = context.get_current_time();
                }
            },
        );
        context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
            move |context, event| {
                if event.current {
                    *start_time_masking_clone.borrow_mut() = context.get_current_time();
                } else {
                    *end_time_masking_clone.borrow_mut() = context.get_current_time();
                }
            },
        );

        let p1 = context.add_person(()).unwrap();
        symptom_init(&mut context).unwrap();
        policy_init(&mut context);
        context.infect_person(p1, None, None, None);
        context.execute();

        let delay = start_time_symptoms.take() - start_time_isolation.take();
        assert!(delay <= maximum_uptake_delay,);
        assert_eq!(*end_time_symptoms.borrow(), *end_time_isolation.borrow(),);
        assert_eq!(*start_time_masking.borrow(), *end_time_symptoms.borrow(),);
        assert_eq!(
            *end_time_masking.borrow(),
            *start_time_masking.borrow() + post_isolation_duration
        );
    }
}
