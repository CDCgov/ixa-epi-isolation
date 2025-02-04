use ixa::{
    define_rng, Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, PersonId,
};
use statrs::distribution::Exp;

use crate::{
    infectiousness_rate::{
        InfectiousnessRate, InfectiousnessRateId, InfectiousnessRateExt, TimeOfInfection,
    },
    population_loader::Household,
    settings::ContextSettingExt,
    Alpha, GlobalTransmissibility, RecoveryTime,
};

define_rng!(InfectionTimeRng);

/// Given intrinsic infectiousness at t, calculate the total infectiousness of a person
/// given other factors related to their setting etc.
pub fn calc_total_infectiousness(
    context: &Context,
    intrinsic: f64,
    person: PersonId,
) -> f64 {
    let household_id = context.get_person_setting_id(person, Household);
    let household_members = context.get_settings_members(Household, household_id).len();
    let max_contacts = (household_members - 1) as f64;
    let alpha = *context.get_global_property_value(Alpha).unwrap();
    intrinsic *  max_contacts.powf(alpha)
}

pub struct Forecast {
    pub next_time: f64,
    pub expected_rate: f64,
}

pub fn get_forecast(context: &Context, current_time: f64, person_id: PersonId) -> Option<Forecast> {
    let index = context
        .get_person_property(person_id, InfectiousnessRateId)
        .unwrap();

    let intrinsic_rate = context.get_max_infection_rate(index);
    let max_time = context.get_max_infection_time(index);

    let rate = calc_total_infectiousness(context, intrinsic_rate, person_id);

    // Because we are using a constant rate for the forecast (max rate), we use
    // an exponential. If we wanted a more sophisticated time-varying forecast,
    // we'd need to use some function of the InfectiousnessRate
    let exp = Exp::new(1.0).unwrap();
    let next_time = current_time + context.sample_distr(InfectionTimeRng, exp) / rate;

    // This should be the forecasted rate at next_time.
    // If the forecast was time-varying this would be different, but
    // because we're using max rate, it's the same
    let intrinsic_at_t = context.get_infection_rate(index, next_time);
    let expected_rate = calc_total_infectiousness(context, intrinsic_at_t, person_id);

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

struct ConstantRate {
    rate: f64,
    global_transmissibility: f64,
    max_time: f64,
}

impl InfectiousnessRate for ConstantRate {
    fn get_rate(&self, _t: f64) -> f64 {
        // This would be fancier if we had a variable rate
        self.rate * self.global_transmissibility
    }
    fn max_rate(&self) -> f64 {
        self.get_rate(0.0)
    }
    fn max_time(&self) -> f64 {
        self.max_time
    }
}

// Eventually, we're actually going to load these in from a file
pub fn init(context: &mut Context) {
    let global_transmissibility = *context
        .get_global_property_value(GlobalTransmissibility)
        .unwrap();
    let max_time = *context.get_global_property_value(RecoveryTime).unwrap();
    context.add_infectiousness_function(Box::new(ConstantRate {
        rate: 1.0,
        global_transmissibility,
        max_time,
    }));
    context.add_infectiousness_function(Box::new(ConstantRate {
        rate: 2.0,
        global_transmissibility,
        max_time,
    }));
}
