use crate::infectiousness_manager::{
    evaluate_forecast, get_forecast, select_next_contact, Forecast, InfectionContextExt,
    InfectionStatus, InfectionStatusValue,
};
use crate::parameters::{ContextParametersExt, Params};
use crate::rate_fns::{ConstantRate, InfectiousnessRateExt};
use ixa::{define_rng, trace, Context, ContextPeopleExt, PersonId, PersonPropertyChangeEvent};

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
                if let Some(next_contact) = select_next_contact(context, person) {
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
    let &Params {
        infection_duration, ..
    } = context.get_params();
    let recovery_time = context.get_current_time() + infection_duration;
    context.add_plan(recovery_time, move |context| {
        trace!("Person {person} has recovered at {recovery_time}");
        context.recover_person(person);
    });
}

/// Load a rate function.
/// TODO<ryl8@cdc.gov>: Eventually, we will load multiple values from a file / files
/// and randomly assign them to people
pub fn load_rate_fns(context: &mut Context) {
    let &Params {
        rate_of_infection,
        infection_duration,
        ..
    } = context.get_params();

    context.add_rate_fn(Box::new(ConstantRate::new(
        rate_of_infection,
        infection_duration,
    )));
}

/// Seeds the initial population with a number of infectious people.
fn seed_infections(context: &mut Context, initial_infections: usize) {
    // First, seed an infectious population
    for _ in 0..initial_infections {
        let person = context.sample_person(InfectionRng, ()).unwrap();
        context.infect_person(person, None);
    }
}

pub fn init(context: &mut Context) {
    let &Params {
        initial_infections, ..
    } = context.get_params();

    load_rate_fns(context);

    // Seed the initial population
    context.add_plan(0.0, move |context| {
        seed_infections(context, initial_infections);
    });

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(|context, event| {
        if event.current != InfectionStatusValue::Infectious {
            return;
        }
        schedule_next_forecasted_infection(context, event.person_id);
        schedule_recovery(context, event.person_id);
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use std::path::PathBuf;

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ExecutionPhase,
    };
    use statrs::assert_almost_eq;

    use crate::{
        infection_propagation_loop::{
            init, load_rate_fns, schedule_next_forecasted_infection, schedule_recovery,
            InfectionStatus, InfectionStatusValue,
        },
        infectiousness_manager::{max_total_infectiousness_multiplier, InfectionContextExt},
        parameters::{ContextParametersExt, GlobalParams, Params},
    };

    use super::seed_infections;

    fn setup_context(seed: u64, rate_of_infection: f64) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed,
            rate_of_infection,
            infection_duration: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context
    }

    #[test]
    fn test_seed_infections() {
        let mut context = setup_context(0, 1.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }
        load_rate_fns(&mut context);
        seed_infections(&mut context, 5);
        let infectious_count = context
            .query_people((InfectionStatus, InfectionStatusValue::Infectious))
            .len();
        assert_eq!(infectious_count, 5);
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context(0, 1.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }

        init(&mut context);

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
    fn avg_number_infections_one_time_unit() {
        // Does one infectious person generate the number of infections as expected by the rate?
        // We're going to run many simulations that each start with one person infected and have a
        // very large number of susceptible people (mirroring one susceptible in an infinitely-large
        // population) but those people themselves cannot transmit secondary infections. We're going
        // to stop those simulations at the end of 1.0 time units and compare the number of infected
        // people to the infectious rate we used to set up the simulation.
        const NUM_SIMS: usize = 10000;
        let rate = 1.5;
        // We need the total infectiousness multiplier for the person.
        let mut total_infectiousness_multiplier = None;
        let mut num_infections_end_one_time_unit = [0usize; NUM_SIMS];
        for (seed, num_infections) in num_infections_end_one_time_unit.iter_mut().enumerate() {
            let mut context = setup_context(seed.try_into().unwrap(), rate);
            context.add_plan_with_phase(1.0, ixa::Context::shutdown, ExecutionPhase::Last);
            for _ in 0..100 {
                context.add_person(()).unwrap();
            }
            // We don't want infectious people beyond our index case to be able to transmit, so we
            // have to do setup on our own since just calling `init` will trigger a watcher for
            // people becoming infectious that lets them transmit.
            load_rate_fns(&mut context);
            let infectious_person = context.add_person(()).unwrap();
            context.infect_person(infectious_person);
            if total_infectiousness_multiplier.is_none() {
                total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                    &context,
                    infectious_person,
                ));
            }
            schedule_next_forecasted_infection(&mut context, infectious_person);
            schedule_recovery(&mut context, infectious_person);
            context.execute();
            let mut infected_count = context
                .query_people((InfectionStatus, InfectionStatusValue::Infected))
                .len();
            // If our initial infection is still infected, we have to subtract one.
            if context.get_person_property(infectious_person, InfectionStatus)
                == InfectionStatusValue::Infected
            {
                infected_count -= 1;
            }
            *num_infections = infected_count;
        }
        #[allow(clippy::cast_precision_loss)]
        let avg_number_infections =
            num_infections_end_one_time_unit.iter().sum::<usize>() as f64 / NUM_SIMS as f64;
        assert_almost_eq!(
            avg_number_infections,
            rate * total_infectiousness_multiplier.unwrap(),
            0.05
        );
    }
}
