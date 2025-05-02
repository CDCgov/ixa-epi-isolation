use crate::infectiousness_manager::{
    evaluate_forecast, get_forecast, infection_attempt, Forecast, InfectionContextExt,
    InfectionStatus, InfectionStatusValue,
};
use crate::parameters::{ContextParametersExt, Params};
use crate::rate_fns::rate_fn_storage::load_rate_fns;
use ixa::{
    define_rng, trace, Context, ContextPeopleExt, IxaError, PersonId, PersonPropertyChangeEvent,
};

define_rng!(InfectionRng);

fn schedule_next_forecasted_infection(context: &mut Context, person: PersonId) {
    if let Some(Forecast {
        next_time,
        forecasted_total_infectiousness,
    }) = get_forecast(context, person)
    {
        context.add_plan(next_time, move |context| {
            // TODO<ryl8@cc.gov>: We will choose a setting here
            if evaluate_forecast(context, person, forecasted_total_infectiousness) {
                if let Some(next_contact) = infection_attempt(context, person) {
                    trace!("Person {person}: Forecast accepted, infecting {next_contact}");
                    context.infect_person(next_contact, Some(person));
                }
            }
            // Continue scheduling forecasts until the person recovers.
            schedule_next_forecasted_infection(context, person);
        });
    }
}

fn schedule_recovery(context: &mut Context, person: PersonId) {
    let infection_duration = context.get_person_rate_fn(person).infection_duration();
    let recovery_time = context.get_current_time() + infection_duration;
    context.add_plan(recovery_time, move |context| {
        trace!("Person {person} has recovered at {recovery_time}");
        context.recover_person(person);
    });
}

/// Seeds the initial population with a number of infectious people.
/// # Errors
/// - If `initial_infections` is greater than the population size.
fn seed_infections(context: &mut Context, initial_infections: usize) -> Result<(), IxaError> {
    for _ in 0..initial_infections {
        let person = context.sample_person(
            InfectionRng,
            (InfectionStatus, InfectionStatusValue::Susceptible),
        );
        match person {
            Some(person) => {
                context.infect_person(person, None);
            }
            None => {
                return Err(IxaError::IxaError("The number of initial infections to seed is greater than the population size. ".to_string() + &format!("The population size is {}, and the number of initial infections to seed is {}. Instead, the entire population was infected.", context.get_current_population(), initial_infections)));
            }
        }
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        initial_infections, ..
    } = context.get_params();

    load_rate_fns(context)?;

    // Seed the initial population
    context.add_plan(0.0, move |context| {
        seed_infections(context, initial_infections).unwrap();
    });

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(|context, event| {
        if event.current != InfectionStatusValue::Infectious {
            return;
        }
        schedule_next_forecasted_infection(context, event.person_id);
        schedule_recovery(context, event.person_id);
    });
    Ok(())
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use std::{any::TypeId, cell::RefCell, path::PathBuf, rc::Rc};

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ExecutionPhase,
        IxaError, PersonId, PersonPropertyChangeEvent,
    };
    use statrs::{
        assert_almost_eq,
        distribution::{ContinuousCDF, Uniform},
    };

    use crate::{
        define_setting_type,
        infection_propagation_loop::{
            init, schedule_next_forecasted_infection, InfectionStatus, InfectionStatusValue,
        },
        infectiousness_manager::{
            max_total_infectiousness_multiplier, InfectionContextExt, InfectionData,
            InfectionDataValue,
        },
        parameters::{
            ContextParametersExt, GlobalParams, ItineraryWriteFnType, Params, RateFnType,
        },
        rate_fns::load_rate_fns,
        settings::{create_itinerary, ContextSettingExt, SettingProperties},
    };

    use super::{schedule_recovery, seed_infections};

    define_setting_type!(HomogeneousMixing);

    fn set_homogenous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = create_itinerary(context, vec![(TypeId::of::<HomogeneousMixing>(), 0)]);
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context(seed: u64, rate: f64, alpha: f64) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed,
            infectiousness_rate_fn: RateFnType::Constant {
                rate,
                duration: 5.0,
            },
            symptom_progression_library: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        context.register_setting_type(HomogeneousMixing {}, SettingProperties { alpha });
        context
    }

    #[test]
    fn test_seed_infections_errors() {
        let mut context = setup_context(0, 1.0, 1.0);
        for _ in 0..3 {
            context.add_person(()).unwrap();
        }
        load_rate_fns(&mut context).unwrap();
        let e = seed_infections(&mut context, 5).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "The number of initial infections to seed is greater than the population size. The population size is 3, and the number of initial infections to seed is 5. Instead, the entire population was infected."
                );
            }
            Some(ue) => panic!(
                "Expected an error that seeding infections should fail because the population size is too small. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, seeded infections with no errors."),
        }
    }

    #[test]
    fn test_seed_infections() {
        let mut context = setup_context(0, 1.0, 1.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }

        load_rate_fns(&mut context).unwrap();
        seed_infections(&mut context, 5).unwrap();
        let infectious_count = context
            .query_people((InfectionStatus, InfectionStatusValue::Infectious))
            .len();
        assert_eq!(infectious_count, 5);
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context(42, 1.0, 1.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        let &Params {
            initial_infections: expected_infectious,
            ..
        } = context.get_params();

        // At the end of 0.0, we should have seeded 3 infections
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                let infectious_count = context
                    .query_people((InfectionStatus, InfectionStatusValue::Infectious))
                    .len();
                assert_eq!(
                    infectious_count, expected_infectious,
                    "Infections should be seeded at 0.0"
                );
            },
            ExecutionPhase::Last,
        );

        context.execute();
        assert!(
            !context
                .query_people((InfectionStatus, InfectionStatusValue::Recovered))
                .is_empty(),
            "Expected some people to recover"
        );
    }

    #[test]
    fn test_zero_rate_no_infections() {
        let mut context = setup_context(0, 0.0, 1.0);
        for _ in 0..=context.get_params().initial_infections {
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        let num_new_infections = Rc::new(RefCell::new(0usize));
        let num_new_infections_clone = Rc::clone(&num_new_infections);
        context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
            move |_context, event| {
                if event.current == InfectionStatusValue::Infectious {
                    *num_new_infections_clone.borrow_mut() += 1;
                }
            },
        );

        context.execute();

        assert_eq!(
            *num_new_infections.borrow(),
            context.get_params().initial_infections
        );
    }

    #[test]
    fn test_number_timing_infections_one_time_unit() {
        // Does one infectious person generate the number of infections as expected by the rate?
        // We're going to run many simulations that each start with one infectious and one
        // susceptible person. The susceptible person gets moved back to susceptible when becoming
        // infected, so this is really a setup where there is no susceptible depletion/an
        // infinitely large starting population. We stop the simulation at the end of 1.0 time units
        // and compare the number of infected people to the infectious rate.
        // We're also going to check the times at which they are infected. In this test simulation,
        // we are using a constant hazard of infection, and we only record infection times that are
        // within 1.0 time units, so we expect the timing of infection attempts to follow U(0, 1).
        // First, we should not expect to observe an exponential distribution because we may observe
        // multiple infection attempts in the same experiment, not just the first. This also helps
        // provide intuition for why we expect a uniform distribution -- if the first infection
        // attempt happens quickly, that increases the chance we see another in 1.0 time units, and
        // because there is basically this compensating relationship between the time and the number
        // of events, they "cancel" each other out to give a uniform distribution (handwavingly).
        let num_sims: u64 = 15_000;
        let rate = 1.5;
        let alpha = 0.42;
        // We need the total infectiousness multiplier for the person.
        let mut total_infectiousness_multiplier = None;
        // Where we store the infection times.
        let infection_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let num_infected = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let infection_times_clone = Rc::clone(&infection_times);
            let num_infected_clone = Rc::clone(&num_infected);
            let mut context = setup_context(seed, rate, alpha);
            // We only run the simulation for 1.0 time units.
            context.add_plan_with_phase(1.0, ixa::Context::shutdown, ExecutionPhase::Last);
            // Add a a person who will get infected.
            let p1 = context.add_person(()).unwrap();
            set_homogenous_mixing_itinerary(&mut context, p1).unwrap();
            // We don't want infectious people beyond our index case to be able to transmit, so we
            // have to do setup on our own since just calling `init` will trigger a watcher for
            // people becoming infectious that lets them transmit.
            load_rate_fns(&mut context).unwrap();
            // Add our infectious fellow.
            let infectious_person = context.add_person(()).unwrap();
            set_homogenous_mixing_itinerary(&mut context, infectious_person).unwrap();

            context.infect_person(infectious_person, None);
            // Get the total infectiousness multiplier for comparison to total number of infections.
            if total_infectiousness_multiplier.is_none() {
                total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                    &context,
                    infectious_person,
                ));
            }
            // Add a watcher for when people are infected to record the infection times.
            context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
                move |context, event| {
                    if event.current == InfectionStatusValue::Infectious {
                        let current_time = context.get_current_time();
                        infection_times_clone.borrow_mut().push(current_time);
                        // Reset the person to susceptible.
                        if event.person_id != infectious_person {
                            *num_infected_clone.borrow_mut() += 1;
                            context.set_person_property(
                                event.person_id,
                                InfectionData,
                                InfectionDataValue::Susceptible,
                            );
                        }
                    }
                },
            );
            // Setup is now over -- onto actually letting our infectious fellow infect others.
            schedule_next_forecasted_infection(&mut context, infectious_person);
            context.execute();
        }
        #[allow(clippy::cast_precision_loss)]
        let avg_number_infections = num_infected.take() as f64 / num_sims as f64;
        assert_almost_eq!(
            avg_number_infections,
            rate * total_infectiousness_multiplier.unwrap(),
            0.05
        );
        // Check whether the times at when people are infected fall uniformly on [0, 1].
        check_ks_stat(&mut infection_times.borrow_mut(), |x| {
            Uniform::new(0.0, 1.0).unwrap().cdf(x)
        });
    }

    fn check_ks_stat(times: &mut [f64], theoretical_cdf: impl Fn(f64) -> f64) {
        // Sort the empirical times to make an empirical CDF.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // KS stat is the maximum observed CDF deviation.
        let ks_stat = times
            .iter()
            .enumerate()
            .map(|(i, time)| {
                #[allow(clippy::cast_precision_loss)]
                let empirical_cdf_value = (i as f64) / (times.len() as f64);
                let theoretical_cdf_value = theoretical_cdf(*time);
                (empirical_cdf_value - theoretical_cdf_value).abs()
            })
            .reduce(f64::max)
            .unwrap();

        assert_almost_eq!(ks_stat, 0.0, 0.01);
    }

    #[test]
    fn test_schedule_recovery() {
        let mut context = setup_context(0, 0.0, 1.0);
        load_rate_fns(&mut context).unwrap();
        let person = context.add_person(()).unwrap();
        seed_infections(&mut context, 1).unwrap();
        // For later, we need to get the recovery time from the rate function.
        let recovery_time = context.get_person_rate_fn(person).infection_duration();
        schedule_recovery(&mut context, person);
        context.execute();
        // Make sure person is recovered.
        assert_eq!(
            context.get_person_property(person, InfectionData),
            InfectionDataValue::Recovered {
                infection_time: 0.0,
                recovery_time
            }
        );
        // Make sure nothing has happened after person is recovered.
        assert_almost_eq!(context.get_current_time(), recovery_time, 0.0);
    }
}
