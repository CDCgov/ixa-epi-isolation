use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextGlobalPropertiesExt,
    ContextPeopleExt, ContextRandomExt, PersonId,
};
use ordered_float::OrderedFloat;
use statrs::distribution::Exp;

use crate::{
    infection_propagation_loop::{InfectionStatus, InfectionStatusValue},
    infectiousness_rate::{
        ConstantRate, InfectiousnessRateExt, InfectiousnessRateFn, ScaledRateFn,
    },
    population_loader::{Alive, Household},
    settings::{ContextSettingExt, Setting},
    Alpha, RecoveryTime, TransmissibilityFactor,
};

define_person_property_with_default!(TimeOfInfection, Option<OrderedFloat<f64>>, None);

/// Calculate the a scaling factor for total infectiousness for a person
/// given factors related to their setting. This implementation is not
/// time-dependent.
///
pub fn calc_total_infectiousness_multiplier<S>(context: &Context, person_id: PersonId, s: S) -> f64
where
    S: Setting + Copy,
{
    let id = context.get_person_setting_id(person_id, s);
    let members = context.get_setting_members(s, id).len();
    #[allow(clippy::cast_precision_loss)]
    let max_contacts = (members - 1) as f64;
    // This needs to be data associated with the setting
    let alpha = *context.get_global_property_value(Alpha).unwrap();
    max_contacts.powf(alpha)
}

// TODO<ryl8@cdc.gov> This should actually take into account *all* settings
// Hard-coded for now.
pub fn max_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    calc_total_infectiousness_multiplier(context, person_id, Household)
}

define_rng!(ForecastRng);

pub struct Forecast {
    pub next_time: f64,
    pub expected_rate: f64,
}

/// Forecast of the next expected infection time, and the expected rate of
/// infection at that time.
pub fn get_forecast(context: &Context, person_id: PersonId) -> Option<Forecast> {
    // Get the person's individual infectiousness rate function
    let rate_fn = context.get_person_rate_fn(person_id);

    // This scales infectiousness by the maximum possible infectiousness across all settings
    let scale_factor = max_total_infectiousness_multiplier(context, person_id);
    // We need to shift the intrinsic infectiousness in time
    // TODO<ryl8@cdc.gov>: this should be passed to the ScaledRateFn object, not inverse_cum function
    let elapsed = context.get_elapsed_infection_time(person_id);
    let total_rate_fn = ScaledRateFn::new(rate_fn, scale_factor);

    trace!("Elapsed={elapsed} Scale={scale_factor}");

    // Because we are using a constant rate for the forecast (max rate), we use
    // an exponential. If we wanted a more sophisticated time-varying forecast,
    // we'd need to use some function of the InfectiousnessRate
    let exp = Exp::new(1.0).unwrap();
    let e = context.sample_distr(ForecastRng, exp);
    // Note: this returns None if forecasted > max_time
    let t = total_rate_fn.inverse_cum(e, elapsed)?;

    let next_time = context.get_current_time() + t;
    let expected_rate = total_rate_fn.get_rate(t);

    trace!("Forecast t={expected_rate} expected_rate={expected_rate}");

    Some(Forecast {
        next_time,
        expected_rate,
    })
}

/// Evaluates a forecast against the actual current infectious,
/// Returns a contact to be infected or None if the forecast is rejected
pub fn evaluate_forecast(
    context: &mut Context,
    person_id: PersonId,
    forecasted_total_infectiousness: f64,
) -> Option<PersonId> {
    // Again, we should probably be offsetting the rate_fn instead
    let elapsed_infection_time = context.get_elapsed_infection_time(person_id);
    let rate_fn = context.get_person_rate_fn(person_id);

    // TODO<ryl8>: The setting is hard-coded for now, but we should replace this
    // with something more sophisticated.
    let current_setting = Household;

    let total_multiplier =
        calc_total_infectiousness_multiplier(context, person_id, current_setting);
    let total_rate_fn = ScaledRateFn::new(rate_fn, total_multiplier);

    let current_infectiousness = total_rate_fn.get_rate(elapsed_infection_time);
    trace!("Current={current_infectiousness} Forecasted={forecasted_total_infectiousness}");

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        if !context.sample_bool(
            ForecastRng,
            current_infectiousness / forecasted_total_infectiousness,
        ) {
            return None;
        }
    }

    context.get_contact(
        person_id,
        current_setting,
        (
            (Alive, true),
            (InfectionStatus, InfectionStatusValue::Susceptible),
        ),
    )
}

pub trait InfectionContextExt {
    fn assign_infection_properties(&mut self, person_id: PersonId);
    fn get_start_of_infection(&self, person_id: PersonId) -> f64;
    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64;
}

impl InfectionContextExt for Context {
    // This function should be called from the main loop whenever
    // someone is first infected. It assigns all their properties needed to
    // calculate intrinsic infectiousness
    fn assign_infection_properties(&mut self, person_id: PersonId) {
        let t = self.get_current_time();
        self.set_person_property(person_id, TimeOfInfection, Some(OrderedFloat(t)));
        self.assign_random_rate_fn(person_id);
    }

    fn get_start_of_infection(&self, person_id: PersonId) -> f64 {
        self.get_person_property(person_id, TimeOfInfection)
            .unwrap()
            .0
    }

    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64 {
        let current_time = self.get_current_time();
        let start_of_infection = self.get_start_of_infection(person_id);
        current_time - start_of_infection
    }
}

// Eventually, we're actually going to load these in from a file
pub fn init(context: &mut Context) {
    let global_transmissibility = *context
        .get_global_property_value(TransmissibilityFactor)
        .unwrap();
    let max_time = *context.get_global_property_value(RecoveryTime).unwrap();

    // This is where you'd actually load in a bunch of curves from files
    // (like with a EmpiricalRate helper) and instantiate them
    let create_rate_fn =
        |rate: f64| Box::new(ConstantRate::new(rate * global_transmissibility, max_time));
    context.add_rate_fn(create_rate_fn(1.0));
    context.add_rate_fn(create_rate_fn(2.0));
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use super::{
        calc_total_infectiousness_multiplier, get_forecast, max_total_infectiousness_multiplier,
        InfectionContextExt, TimeOfInfection,
    };
    use crate::{
        infectiousness_rate::{ConstantRate, InfectiousnessRateExt},
        population_loader::{Household, HouseholdSettingId},
        Alpha, RecoveryTime, TransmissibilityFactor,
    };
    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt};
    use ordered_float::OrderedFloat;

    fn setup_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
            .set_global_property_value(TransmissibilityFactor, 1.0)
            .unwrap();
        context
            .set_global_property_value(RecoveryTime, 3.0)
            .unwrap();
        context.add_rate_fn(Box::new(ConstantRate::new(1.0, 3.0)));
        context
    }

    #[test]
    fn test_get_start_of_infection() {
        let mut context = setup_context();
        let p1 = context.add_person((HouseholdSettingId, 1)).unwrap();
        context.set_person_property(p1, TimeOfInfection, Some(OrderedFloat(2.0)));
        assert_eq!(context.get_start_of_infection(p1), 2.0);
    }

    #[test]
    fn test_get_elapsed_infection_time() {
        let mut context = setup_context();
        let p1 = context.add_person((HouseholdSettingId, 1)).unwrap();
        context.set_person_property(p1, TimeOfInfection, Some(OrderedFloat(1.0)));
        context.add_plan(3.0, move |context| {
            assert_eq!(context.get_elapsed_infection_time(p1), 2.0);
        });
        context.execute();
    }

    #[test]
    fn test_assign_infection_properties() {
        let mut context = setup_context();
        let p1 = context.add_person((HouseholdSettingId, 1)).unwrap();
        context.add_plan(1.0, move |context| {
            context.assign_infection_properties(p1);
            let _ = context.get_person_rate_fn(p1);
            assert_eq!(context.get_start_of_infection(p1), 1.0);
        });
        context.execute();
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier() {
        let mut context = setup_context();
        context.set_global_property_value(Alpha, 0.5).unwrap();
        let p1 = context.add_person((HouseholdSettingId, 1)).unwrap();
        // Add two additional contacts, which should make the factor 2
        context.add_person((HouseholdSettingId, 1)).unwrap();
        context.add_person((HouseholdSettingId, 1)).unwrap();

        // Number of additional contacts ^ Alpha
        let expected = 2.0_f64.powf(0.5);
        assert_eq!(
            calc_total_infectiousness_multiplier(&context, p1, Household),
            expected
        );
        // For now, max is always just the value of the Household infectiousness
        assert_eq!(max_total_infectiousness_multiplier(&context, p1), expected);
    }

    #[test]
    fn test_forecast() {
        let mut context = setup_context();
        context.set_global_property_value(Alpha, 1.0).unwrap();
        let p1 = context.add_person((HouseholdSettingId, 1)).unwrap();
        // Add two additional contacts, which should make the factor 2
        context.add_person((HouseholdSettingId, 1)).unwrap();
        context.add_person((HouseholdSettingId, 1)).unwrap();

        context.assign_infection_properties(p1);

        let f = get_forecast(&context, p1).expect("Forecast should be returned");
        // The expected rate is 2.0, because intrinsic is 1.0 and there are 2 contacts.
        // TODO<ryl8@cdc>: Check if the times are reasonable
        assert_eq!(f.expected_rate, 2.0);
    }
}
