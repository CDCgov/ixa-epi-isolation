use crate::infectiousness_setup::{evaluate_forecast, get_forecast, Forecast, InfectionContextExt};
use crate::InitialInfections;
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
    let forecast = get_forecast(context, person);
    if forecast.is_none() {
        // Note: If the forecast returns None because the person lives alone
        // (i.e., total infectiousness multiplier is 0) this isn't quite right
        trace!("Person {person} has recovered at {current_time}");
        context.set_person_property(person, InfectionStatus, InfectionStatusValue::Recovered);
        return;
    }
    let Forecast {
        next_time,
        forecasted_total_infectiousness,
    } = forecast.unwrap();
    context.add_plan(next_time, move |context| {
        let next_contact = evaluate_forecast(context, person, forecasted_total_infectiousness);
        if let Some(next_contact) = next_contact {
            context.set_person_property(
                next_contact,
                InfectionStatus,
                InfectionStatusValue::Infected,
            );
        }
        // Right now, forecasts will continue until the person recovers, regardless
        // of if there are any more contacts left to infect.
        schedule_next_forecasted_infection(context, person);
    });
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
    let initial_infections = *context
        .get_global_property_value(InitialInfections)
        .unwrap();

    // Seed the initial population
    context.add_plan(0.0, move |context| {
        seed_infections(context, initial_infections);
    });

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(|context, event| {
        if event.current == InfectionStatusValue::Infected {
            trace!(
                "Person {}: Infected at {}",
                event.person_id,
                context.get_current_time()
            );
            context.assign_infection_properties(event.person_id);
            schedule_next_forecasted_infection(context, event.person_id);
        }
    });
}
