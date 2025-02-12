use crate::infectiousness_manager::{
    evaluate_forecast, get_forecast, select_next_contact, Forecast, InfectionContextExt,
};
use crate::parameters::Parameters;
use crate::rate_fns::{ConstantRate, InfectiousnessRateExt};
use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextGlobalPropertiesExt,
    ContextPeopleExt, PersonId, PersonPropertyChangeEvent,
};

#[derive(Hash, PartialEq, Debug, Clone, Copy)]
pub enum InfectionStatusValue {
    Susceptible,
    Infected,
    Recovered,
}
define_person_property_with_default!(
    InfectionStatus,
    InfectionStatusValue,
    InfectionStatusValue::Susceptible
);

define_rng!(InfectionRng);

fn schedule_next_forecasted_infection(context: &mut Context, person: PersonId) {
    let current_time = context.get_current_time();
    match get_forecast(context, person) {
        None => {
            // No forecast was returned, so the person is assumed to recover.
            // Note: this may not be quite right if  total infectiousness multiplier is 0
            // e.g., because the person is alone
            trace!("Person {person} has recovered at {current_time}");
            context.set_person_property(person, InfectionStatus, InfectionStatusValue::Recovered);
        }
        Some(Forecast {
            next_time,
            forecasted_total_infectiousness,
        }) => {
            context.add_plan(next_time, move |context| {
                // TODO<ryl8@cc.gov>: We will choose a setting here
                if evaluate_forecast(context, person, forecasted_total_infectiousness) {
                    if let Some(next_contact) = select_next_contact(context, person) {
                        trace!("Person {person}: Forecast accepted, infecting {next_contact}");
                        context.set_person_property(
                            next_contact,
                            InfectionStatus,
                            InfectionStatusValue::Infected,
                        );
                    }
                }
                // Continue scheduling forecasts until the person recovers.
                schedule_next_forecasted_infection(context, person);
            });
        }
    }
}

/// Load the set of rate functions we will randomly assign to people
/// Eventually, these will actually be loaded from a file
pub fn load_rate_fns(context: &mut Context) {
    let parameters = context.get_global_property_value(Parameters).unwrap();
    let global_transmissibility = parameters.global_transmissibility;
    let max_time = parameters.infection_duration;

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
        context.set_person_property(person, InfectionStatus, InfectionStatusValue::Infected);
    }
}

pub fn init(context: &mut Context) {
    let parameters = context.get_global_property_value(Parameters).unwrap();
    let initial_infections = parameters.initial_infections;

    load_rate_fns(context);

    // Seed the initial population
    context.add_plan(0.0, move |context| {
        seed_infections(context, initial_infections);
    });

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(|context, event| {
        if event.current != InfectionStatusValue::Infected {
            return;
        }

        let person = event.person_id;
        let t = context.get_current_time();
        trace!("Person {person}: Infected at {t}");

        context.assign_infection_properties(event.person_id);

        schedule_next_forecasted_infection(context, person);
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
        infection_propagation_loop::{init, InfectionStatus, InfectionStatusValue},
        parameters::{Parameters, ParametersValues},
        population_loader::CensusTract,
    };

    use super::seed_infections;

    fn setup_context() -> Context {
        let mut context = Context::new();
        let parameters = ParametersValues {
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
            .set_global_property_value(Parameters, parameters)
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
            .query_people((InfectionStatus, InfectionStatusValue::Infected))
            .len();
        assert_eq!(infected_count, 5);
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context();
        for _ in 0..10 {
            context.add_person((CensusTract, 1)).unwrap();
        }

        init(&mut context);

        let parameters = context.get_global_property_value(Parameters).unwrap();
        let expected_infected = parameters.initial_infections;

        // At the end of 0.0, we should have seeded 3 infections
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                let infected_count = context
                    .query_people((InfectionStatus, InfectionStatusValue::Infected))
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
                .query_people((InfectionStatus, InfectionStatusValue::Recovered))
                .is_empty(),
            "Expected some people to recover"
        );
    }
}
