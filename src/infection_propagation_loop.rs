use crate::infectiousness_manager::{self, Forecast, InfectionContextExt};
use crate::parameters::{ContextParametersExt, Params};
use crate::rate_fns::{ConstantRate, InfectiousnessRateExt};
use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt, PersonId,
    PersonPropertyChangeEvent,
};

#[derive(Hash, PartialEq, Debug, Clone, Copy)]
pub enum InfectiousStatusValue {
    Susceptible,
    Infected,
    Recovered,
}
define_person_property_with_default!(
    InfectiousStatus,
    InfectiousStatusValue,
    InfectiousStatusValue::Susceptible
);

define_rng!(InfectionRng);

fn schedule_next_forecasted_infection(
    context: &mut Context,
    person: PersonId,
    get_forecast: impl Fn(&Context, PersonId) -> Option<Forecast> + 'static,
    evaluate_forecast: impl Fn(&mut Context, PersonId, f64) -> Option<PersonId> + 'static,
) {
    let current_time = context.get_current_time();
    let forecast = get_forecast(context, person);
    if forecast.is_none() {
        // Note: If the forecast returns None because the person lives alone
        // (i.e., total infectiousness multiplier is 0) this isn't quite right
        trace!("Person {person} has recovered at {current_time}");
        context.set_person_property(person, InfectiousStatus, InfectiousStatusValue::Recovered);
        return;
    }
    let Forecast {
        next_time,
        forecasted_total_infectiousness,
    } = forecast.unwrap();
    context.add_plan(next_time, move |context| {
        let next_contact = evaluate_forecast(context, person, forecasted_total_infectiousness);
        if let Some(next_contact) = next_contact {
            trace!("Person {person}: Forecast accepted, infecting {next_contact}");
            context.set_person_property(
                next_contact,
                InfectiousStatus,
                InfectiousStatusValue::Infected,
            );
        }
        // Right now, forecasts will continue until the person recovers, regardless
        // of if there are any more contacts left to infect.
        schedule_next_forecasted_infection(context, person, get_forecast, evaluate_forecast);
    });
}

/// Load the set of rate functions we will randomly assign to people
/// Eventually, these will actually be loaded from a file
pub fn load_rate_fns(context: &mut Context) {
    let &Params {
        global_transmissibility,
        max_time,
        ..
    } = context.get_params();

    let create_rate_fn =
        |rate: f64| Box::new(ConstantRate::new(rate * global_transmissibility, max_time));
    context.add_rate_fn(create_rate_fn(1.0));
    context.add_rate_fn(create_rate_fn(2.0));
}

/// Seeds the initial population with a number of infected people.
fn seed_infections(context: &mut Context, initial_infections: usize) {
    // First, seed an infected population
    for _ in 0..initial_infections {
        let person = context.sample_person(InfectionRng, ()).unwrap();
        context.set_person_property(person, InfectiousStatus, InfectiousStatusValue::Infected);
    }
}

pub fn init(context: &mut Context) {
    let &Params {
        initial_infections,
        max_time,
        ..
    } = context.get_params();

    load_rate_fns(context);

    // Seed the initial population
    context.add_plan(0.0, move |context| {
        seed_infections(context, initial_infections);
    });

    // Add a plan to shut down the simulation after `max_time`, regardless of
    // what else is happening in the model.
    context.add_plan(max_time, |context| {
        context.shutdown();
    });

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectiousStatus>>(|context, event| {
        if event.current != InfectiousStatusValue::Infected {
            return;
        }

        let person = event.person_id;
        let t = context.get_current_time();
        trace!("Person {person}: Infected at {t}");

        context.assign_infection_properties(event.person_id);

        schedule_next_forecasted_infection(
            context,
            person,
            infectiousness_manager::get_forecast,
            infectiousness_manager::evaluate_forecast,
        );
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
        contact::ContextContactExt,
        infection_propagation_loop::{
            init, schedule_next_forecasted_infection, InfectiousStatus, InfectiousStatusValue,
        },
        infectiousness_manager::Forecast,
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::CensusTract,
    };

    use super::seed_infections;

    fn setup_context() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            global_transmissibility: 1.0,
            infection_duration: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
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
        seed_infections(&mut context, 5);
        let infected_count = context
            .query_people((InfectiousStatus, InfectiousStatusValue::Infected))
            .len();
        assert_eq!(infected_count, 5);
    }

    #[test]
    fn test_schedule_next_forecasted_infection() {
        let mut context = setup_context();
        let person = context.add_person(()).unwrap();
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }
        context.set_person_property(person, InfectiousStatus, InfectiousStatusValue::Infected);

        schedule_next_forecasted_infection(
            &mut context,
            person,
            |context, _| {
                // Person should recover at time 9.0 after infecting 2 other people
                if context.get_current_time() >= 9.0 {
                    return None;
                }
                Some(Forecast {
                    // Person should be scheduled to attempt infection at 3.0
                    next_time: context.get_current_time() + 3.0,
                    forecasted_total_infectiousness: 1.0,
                })
            },
            move |context, _, _| context.get_contact(person, ()),
        );

        context.execute();

        assert_eq!(context.get_current_time(), 9.0);
        assert_eq!(
            context.get_person_property(person, InfectiousStatus),
            InfectiousStatusValue::Recovered
        );
        assert_eq!(
            context
                .query_people((InfectiousStatus, InfectiousStatusValue::Infected))
                .len(),
            3
        );
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context();
        for _ in 0..10 {
            context.add_person((CensusTract, 1)).unwrap();
        }

        init(&mut context);

        let expected_infected = context.get_params().initial_infections;

        // At the end of 0.0, we should have seeded 3 infections
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                let infected_count = context
                    .query_people((InfectiousStatus, InfectiousStatusValue::Infected))
                    .len();
                assert_eq!(
                    infected_count, expected_infected,
                    "Infections should be seeded at 0.0"
                );
            },
            ExecutionPhase::Last,
        );

        context.execute();
        assert!(
            !context
                .query_people((InfectiousStatus, InfectiousStatusValue::Recovered))
                .is_empty(),
            "Expected some people to recover"
        );
    }
}
