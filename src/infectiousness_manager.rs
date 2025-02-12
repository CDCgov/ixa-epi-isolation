use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonId,
};
use serde::Serialize;
use statrs::distribution::Exp;

use crate::{
    contact::ContextContactExt,
    population_loader::Alive,
    rate_fns::{InfectiousnessRateExt, InfectiousnessRateFn, RateFnId, ScaledRateFn},
};

#[derive(Serialize, PartialEq, Debug, Clone, Copy)]
pub enum InfectionDataValue {
    Susceptible,
    Infected {
        infection_time: f64,
        rate_fn_id: RateFnId,
    },
    Recovered {
        recovery_time: f64,
    },
}

#[derive(Serialize, PartialEq, Debug, Clone, Copy)]
pub enum InfectionStatusValue {
    Susceptible,
    Infected,
    Recovered,
}

define_person_property_with_default!(
    InfectionData,
    InfectionDataValue,
    InfectionDataValue::Susceptible
);

define_derived_property!(
    InfectionStatus,
    InfectionStatusValue,
    [InfectionData],
    |data| match data {
        InfectionDataValue::Susceptible => InfectionStatusValue::Susceptible,
        InfectionDataValue::Infected { .. } => InfectionStatusValue::Infected,
        InfectionDataValue::Recovered { .. } => InfectionStatusValue::Recovered,
    }
);

const TOTAL_INFECTIOUSNESS_MULTIPLIER: f64 = 2.0;

/// Calculate the scaling factor that accounts for the total infectiousness
/// for a person, given factors related to their environment, such as the number of people
/// they come in contact with or how close they are.
/// This is used to scale the intrinsic infectiousness function of that person.
pub fn calc_total_infectiousness_multiplier(_context: &Context, _person_id: PersonId) -> f64 {
    // TODO<ryl8@cdc.gov> This is a placeholder until we have an implementation
    // of settings and itineraries
    TOTAL_INFECTIOUSNESS_MULTIPLIER
}

/// Calculate the maximum possible scaling factor for total infectiousness
/// for a person, given information we know at the time of a forecast.
pub fn max_total_infectiousness_multiplier(_context: &Context, _person_id: PersonId) -> f64 {
    // TODO<ryl8@cdc.gov> This is a placeholder until we have an implementation
    // of settings and itineraries
    TOTAL_INFECTIOUSNESS_MULTIPLIER
}

define_rng!(ForecastRng);

pub struct Forecast {
    pub next_time: f64,
    pub forecasted_total_infectiousness: f64,
}

/// Forecast of the next expected infection time, and the expected rate of
/// infection at that time.
pub fn get_forecast(context: &Context, person_id: PersonId) -> Option<Forecast> {
    // Get the person's individual infectiousness
    let rate_fn = context.get_person_rate_fn(person_id);
    // This scales infectiousness by the maximum possible infectiousness across all settings
    let scale = max_total_infectiousness_multiplier(context, person_id);
    let elapsed = context.get_elapsed_infection_time(person_id);
    let total_rate_fn = ScaledRateFn::new(rate_fn, scale, elapsed);

    // Draw an exponential and use that to determine the next time
    let exp = Exp::new(1.0).unwrap();
    let e = context.sample_distr(ForecastRng, exp);
    // Note: this returns None if forecasted > infectious period
    let t = total_rate_fn.inverse_cum_rate(e)?;

    let next_time = context.get_current_time() + t;
    let forecasted_total_infectiousness = total_rate_fn.rate(t);

    Some(Forecast {
        next_time,
        forecasted_total_infectiousness,
    })
}

/// Evaluates a forecast against the actual current infectious,
/// Returns a contact to be infected or None if the forecast is rejected
pub fn evaluate_forecast(
    context: &mut Context,
    person_id: PersonId,
    forecasted_total_infectiousness: f64,
) -> bool {
    let rate_fn = context.get_person_rate_fn(person_id);

    let total_multiplier = calc_total_infectiousness_multiplier(context, person_id);
    let total_rate_fn = ScaledRateFn::new(rate_fn, total_multiplier, 0.0);

    let elapsed_t = context.get_elapsed_infection_time(person_id);
    let current_infectiousness = total_rate_fn.rate(elapsed_t);

    assert!(
        (current_infectiousness <= forecasted_total_infectiousness),
        "Person {person_id}: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: {current_infectiousness}, Forecasted: {forecasted_total_infectiousness}"
    );

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        if !context.sample_bool(
            ForecastRng,
            current_infectiousness / forecasted_total_infectiousness,
        ) {
            trace!("Person{person_id}: Forecast rejected");

            return false;
        }
    }

    true
}

// TODO<ryl8@cdc.gov>:
/// Choose a the next contact for a given transmitter.
/// Returns None if the contact is not susceptible.
pub fn select_next_contact(context: &Context, person_id: PersonId) -> Option<PersonId> {
    let next_contact = context.get_contact(person_id, ((Alive, true),))?;

    if context.get_person_property(next_contact, InfectionStatus)
        != InfectionStatusValue::Susceptible
    {
        return None;
    }
    Some(next_contact)
}

pub trait InfectionContextExt {
    fn infect_person(&mut self, person_id: PersonId);
    fn recover_person(&mut self, person_id: PersonId);
    fn get_start_of_infection(&self, person_id: PersonId) -> f64;
    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64;
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn;
}

impl InfectionContextExt for Context {
    // This function should be called from the main loop whenever
    // someone is first infected. It assigns all their properties needed to
    // calculate intrinsic infectiousness
    fn infect_person(&mut self, person_id: PersonId) {
        let infection_time = self.get_current_time();
        let rate_fn_id = self.get_random_rate_function();
        trace!("Person {person_id}: Infected at {infection_time}");
        self.set_person_property(
            person_id,
            InfectionData,
            InfectionDataValue::Infected {
                infection_time,
                rate_fn_id,
            },
        );
    }

    fn recover_person(&mut self, person_id: PersonId) {
        let recovery_time = self.get_current_time();
        self.set_person_property(
            person_id,
            InfectionData,
            InfectionDataValue::Recovered { recovery_time },
        );
    }

    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn {
        let InfectionDataValue::Infected { rate_fn_id, .. } =
            self.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infected")
        };
        self.get_rate_fn(rate_fn_id)
    }

    fn get_start_of_infection(&self, person_id: PersonId) -> f64 {
        let InfectionDataValue::Infected { infection_time, .. } =
            self.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infected")
        };
        infection_time
    }

    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64 {
        self.get_current_time() - self.get_start_of_infection(person_id)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use std::path::PathBuf;

    use super::{
        evaluate_forecast, get_forecast, max_total_infectiousness_multiplier, InfectionContextExt,
    };
    use crate::{
        infectiousness_manager::TOTAL_INFECTIOUSNESS_MULTIPLIER,
        parameters::{Parameters, ParametersValues},
        population_loader::CensusTract,
        rate_fns::{ConstantRate, InfectiousnessRateExt},
    };
    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt};

    fn setup_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
            .set_global_property_value(
                Parameters,
                ParametersValues {
                    initial_infections: 1,
                    max_time: 10.0,
                    seed: 0,
                    rate_of_infection: 1.0,
                    infection_duration: 5.0,
                    report_period: 1.0,
                    synth_population_file: PathBuf::from("."),
                },
            )
            .unwrap();
        context.add_rate_fn(Box::new(ConstantRate::new(1.0, 5.0)));
        context
    }

    #[test]
    fn test_infect_person() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1);
        });
        context.execute();
        assert_eq!(context.get_start_of_infection(p1), 2.0);
        context.get_person_rate_fn(p1);
    }

    #[test]
    fn test_get_elapsed_infection_time() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1);
        });
        context.add_plan(3.0, move |_| {});
        context.execute();
        assert_eq!(context.get_elapsed_infection_time(p1), 1.0);
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        // For now, max is always just the value of the CensusTract infectiousness
        assert_eq!(
            max_total_infectiousness_multiplier(&context, p1),
            TOTAL_INFECTIOUSNESS_MULTIPLIER
        );
    }

    #[test]
    fn test_forecast() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        // Add two additional contacts, which should make the factor 2
        context.add_person((CensusTract, 1)).unwrap();
        context.add_person((CensusTract, 1)).unwrap();

        context.infect_person(p1);

        let f = get_forecast(&context, p1).expect("Forecast should be returned");
        // The expected rate is 2.0, because intrinsic is 1.0 and there are 2 contacts.
        // TODO<ryl8@cdc>: Check if the times are reasonable
        assert_eq!(f.forecasted_total_infectiousness, 2.0);
    }

    #[test]
    #[should_panic = "Person 0: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: 2, Forecasted: 1.9"]
    fn test_assert_evaluate_fails_when_forecast_smaller() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.infect_person(p1);

        let invalid_forecast = TOTAL_INFECTIOUSNESS_MULTIPLIER - 0.1;
        evaluate_forecast(&mut context, p1, invalid_forecast);
    }
}
