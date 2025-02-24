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
                    context.create_transmission_event(person, next_contact);
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
        context.infect_person(person);
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

    use crate::{
        infection_propagation_loop::{init, load_rate_fns, InfectionStatus, InfectionStatusValue},
        parameters::{ContextParametersExt, GlobalParams, Params},
    };

    use super::seed_infections;

    fn setup_context() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            rate_of_infection: 1.0,
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
        let mut context = setup_context();
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
        let mut context = setup_context();
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
}
