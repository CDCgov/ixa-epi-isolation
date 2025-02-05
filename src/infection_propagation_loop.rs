use crate::contact::ContextContactExt;
use crate::infectiousness_rate::InfectiousnessRateExt;
use crate::infectiousness_setup::{
    assign_infection_properties, calc_total_infectiousness, get_forecast, Forecast,
};
use crate::population_loader::Household;
use crate::settings::ContextSettingExt;
use crate::InitialInfections;
use ixa::{
    define_person_property_with_default, define_rng, Context, ContextGlobalPropertiesExt,
    ContextPeopleExt, ContextRandomExt, PersonId, PersonPropertyChangeEvent,
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

fn schedule_next_forecasted_infection(
    context: &mut Context,
    person: PersonId,
    reference_time: f64,
) {
    let forecast = get_forecast(context, reference_time, person);
    if forecast.is_none() {
        println!("Person {person} has recovered at {reference_time}");
        context.set_person_property(person, InfectionStatus, InfectionStatusValue::Recovered);
        return;
    }
    let Forecast {
        next_time,
        expected_rate,
    } = forecast.unwrap();
    context.add_plan(next_time, move |context| {
        evaluate_forecast(context, person, expected_rate);
    });
}

fn evaluate_forecast(
    context: &mut Context,
    person: PersonId,
    forecasted_total_infectiousness: f64,
) {
    let current_time = context.get_current_time();
    let rate_fn = context.get_person_rate_fn(person);

    let intrinsic = rate_fn.get_rate(current_time);
    let current_infectiousness = calc_total_infectiousness(context, intrinsic, person);

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        let r = context.sample_range(InfectionRng, 0.0..1.0);
        if r > current_infectiousness / forecasted_total_infectiousness {
            return;
        }
    }

    // Accept the infection
    let household_id = context.get_person_setting_id(person, Household);
    let contact = context.get_contact(person, household_id);
    if contact.is_none() {
        // No more people to infect; exit the loop
        return;
    }
    context.set_person_property(
        contact.unwrap(),
        InfectionStatus,
        InfectionStatusValue::Infected,
    );
    schedule_next_forecasted_infection(context, person, current_time);
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
            println!(
                "Person({}): Infected at {}",
                event.person_id,
                context.get_current_time()
            );
            assign_infection_properties(context, event.person_id);
            schedule_next_forecasted_infection(
                context,
                event.person_id,
                context.get_current_time(),
            );
        }
    });
}
