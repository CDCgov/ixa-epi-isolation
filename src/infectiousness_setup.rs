use ixa::{
    define_person_property_with_default, define_rng, Context, ContextGlobalPropertiesExt,
    ContextPeopleExt, ContextRandomExt, PersonId,
};
use ordered_float::OrderedFloat;
use statrs::distribution::Exp;

use crate::{
    infectiousness_rate::{ConstantRate, InfectiousnessRateExt},
    population_loader::Household,
    settings::ContextSettingExt,
    Alpha, GlobalTransmissibility, RecoveryTime,
};

define_person_property_with_default!(TimeOfInfection, Option<OrderedFloat<f64>>, None);

/// Given some intrinsic infectiousness rate, calculate the total rate of infectiousness
/// given other factors related to their setting. This implementation is not
/// time-dependent.
pub fn calc_total_infectiousness(context: &Context, intrinsic: f64, person: PersonId) -> f64 {
    let household_id = context.get_person_setting_id(person, Household);
    let household_members = context.get_setting_members(Household, household_id).len();
    #[allow(clippy::cast_precision_loss)]
    let max_contacts = (household_members - 1) as f64;
    let alpha = *context.get_global_property_value(Alpha).unwrap();
    intrinsic * max_contacts.powf(alpha)
}

define_rng!(ForecastRng);

pub struct Forecast {
    pub next_time: f64,
    pub expected_rate: f64,
}

/// Forecast of the next expected infection time, and the expected rate of
/// infection at that time.
pub fn get_forecast(context: &Context, current_time: f64, person_id: PersonId) -> Option<Forecast> {
    let rate_fn = context.get_person_rate_fn(person_id);

    let intrinsic_max = rate_fn.max_rate();
    let max_time = rate_fn.max_time();

    // Because we are using a constant rate for the forecast (max rate), we use
    // an exponential. If we wanted a more sophisticated time-varying forecast,
    // we'd need to use some function of the InfectiousnessRate
    let rate = calc_total_infectiousness(context, intrinsic_max, person_id);
    let exp = Exp::new(1.0).unwrap();
    let next_time = current_time + context.sample_distr(ForecastRng, exp) / rate;

    // This should be the forecasted rate at next_time.
    // If the forecast was time-varying this would be different, but
    // because we're using max rate, it's the same
    let forecasted_at_t = rate_fn.max_rate();
    let expected_rate = calc_total_infectiousness(context, forecasted_at_t, person_id);

    // If the next time is past the max infection time for the person,
    // we should not schedule a forecast
    let day_0 = context
        .get_person_property(person_id, TimeOfInfection)
        .unwrap()
        .0;
    if day_0 + next_time >= max_time {
        None
    } else {
        Some(Forecast {
            next_time,
            expected_rate,
        })
    }
}
// This function should be called from the main loop whenever
// someone is first infected. It assigns all their properties needed to
// calculate intrinsic infectiousness
pub fn assign_infection_properties(context: &mut Context, person_id: PersonId) {
    let t = context.get_current_time();
    context.set_person_property(person_id, TimeOfInfection, Some(OrderedFloat(t)));
    context.assign_random_rate_fn(person_id);
}

// Eventually, we're actually going to load these in from a file
pub fn init(context: &mut Context) {
    let global_transmissibility = *context
        .get_global_property_value(GlobalTransmissibility)
        .unwrap();
    let max_time = *context.get_global_property_value(RecoveryTime).unwrap();
    let create_rate_fn =
        |rate: f64| Box::new(ConstantRate::new(rate * global_transmissibility, max_time));
    context.add_rate_fn(create_rate_fn(1.0));
    context.add_rate_fn(create_rate_fn(2.0));
}
