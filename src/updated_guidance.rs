use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonId, PersonPropertyChangeEvent,
};

use statrs::distribution::Uniform;

use crate::{
    parameters::{ContextParametersExt, Params},
    settings::{
        CensusTract, ContextSettingExt, Home, ItineraryEntry, ItineraryModifiers, SettingId,
    },
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(SocialDistanceStatus, bool, false);
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
    let &Params {
        post_isolation_duration,
        uptake_probability,
        maximum_uptake_delay,
        ..
    } = context.get_params();

    // println!(
    //     "Isolation guidance parameters: post_isolation_duration: {}, uptake_probability: {}, maximum_uptake_delay: {}, facemask_transmission_modifier: {}, isolation_transmission_modifier: {}",
    //     post_isolation_duration,
    //     uptake_probability,
    //     maximum_uptake_delay,
    //     facemask_transmission_modifier,
    //     isolation_transmission_modifier,
    // );
    setup_isolation_guidance_event_sequence(
        context,
        post_isolation_duration,
        uptake_probability,
        maximum_uptake_delay,
    );
}

fn social_distance_person(context: &mut Context, person: PersonId, isolation_status: bool) {
    if isolation_status {
        // let home_id = context.get_setting_id(person, &Home);
        let isolation_itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
        ];
        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::Replace {
                itinerary: isolation_itinerary,
            },
        );

        context.set_person_property(person, SocialDistanceStatus, true);
    } else {
        let _ = context.remove_modified_itinerary(person);
        context.set_person_property(person, SocialDistanceStatus, false);
    }
}

fn isolate_person(context: &mut Context, person: PersonId, social_distance_status: bool) {
    if social_distance_status {
        let _ = context.modify_itinerary(person, ItineraryModifiers::Isolate { setting: &Home });
        context.set_person_property(person, IsolatingStatus, true);
    } else {
        let _ = context.remove_modified_itinerary(person);
        context.set_person_property(person, IsolatingStatus, false);
    }
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
                            isolate_person(context, event.person_id, true);
                            trace!("Person {} is now isolating", event.person_id);
                        }
                    });
                }
            }
            None => {
                if context.get_person_property(event.person_id, IsolatingStatus) {
                    isolate_person(context, event.person_id, false);
                    social_distance_person(context, event.person_id, true);
                    trace!(
                        "Person {} is now social distancing and no longer isolating",
                        event.person_id
                    );
                    let t = context.get_current_time();
                    context.add_plan(t + post_isolation_duration, move |context| {
                        social_distance_person(context, event.person_id, false);
                        trace!("Person {} is now not social distancing", event.person_id);
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

    use super::{IsolatingStatus, SocialDistanceStatus};

    fn setup_context(
        post_isolation_duration: f64,
        uptake_probability: f64,
        maximum_uptake_delay: f64,
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
            post_isolation_duration,
            uptake_probability,
            maximum_uptake_delay,
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
        let mut context = setup_context(
            post_isolation_duration,
            uptake_probability,
            maximum_uptake_delay,
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
        context.subscribe_to_event::<PersonPropertyChangeEvent<SocialDistanceStatus>>(
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
