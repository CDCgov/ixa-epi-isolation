use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextGlobalPropertiesExt,
    ContextPeopleExt, ContextRandomExt, PersonId,
};
use ordered_float::OrderedFloat;
use statrs::distribution::Exp;

use crate::{
    infection_propagation_loop::{InfectiousStatus, InfectiousStatusValue},
    parameters::Parameters,
    population_loader::{Alive, CensusTract},
    settings::{ContextSettingExt, Setting},
};

// This needs to be data associated with the setting
const ALPHA: f64 = 1.0;

define_person_property_with_default!(TimeOfInfection, Option<OrderedFloat<f64>>, None);
// This will be replaced with a reference to a rate functions
define_person_property_with_default!(InfectiousnessRate, Option<OrderedFloat<f64>>, None);

/// Calculate the a scaling factor for total infectiousness for a person
/// given factors related to their setting. This implementation is not
/// time-dependent.
///
pub fn calc_setting_infectiousness_multiplier<S>(
    context: &Context,
    person_id: PersonId,
    s: S,
) -> f64
where
    S: Setting + Copy,
{
    let id = context.get_person_setting_id(person_id, s);
    let members = context.get_setting_members(s, id, ()).len();
    #[allow(clippy::cast_precision_loss)]
    let max_contacts = (members - 1) as f64;
    max_contacts.powf(ALPHA)
}

// TODO<ryl8@cdc.gov> This should actually take into account *all* settings
// For now we use your census tract
pub fn max_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    calc_setting_infectiousness_multiplier(context, person_id, CensusTract)
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
    let rate = context
        .get_person_property(person_id, InfectiousnessRate)
        .unwrap()
        .0;
    // This scales infectiousness by the maximum possible infectiousness across all settings
    let scale_factor = max_total_infectiousness_multiplier(context, person_id);
    let forecasted_total_infectiousness = rate * scale_factor;

    let elapsed = context.get_elapsed_infection_time(person_id);

    // Draw an exponential and use that to determine the next time
    let exp = Exp::new(1.0).unwrap();
    let e = context.sample_distr(ForecastRng, exp);
    let t = e / forecasted_total_infectiousness;

    let next_time = context.get_current_time() + t;
    if elapsed + t
        > context
            .get_global_property_value(Parameters)
            .unwrap()
            .infection_duration
    {
        return None;
    }

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
) -> Option<PersonId> {
    let rate = context
        .get_person_property(person_id, InfectiousnessRate)
        .unwrap()
        .0;

    // TODO<ryl8>: The setting is hard-coded for now, but we should replace this
    // with something more sophisticated.
    let current_setting = CensusTract;

    let total_multiplier =
        calc_setting_infectiousness_multiplier(context, person_id, current_setting);
    let total_rate = rate * total_multiplier;

    let _elapsed_t = context.get_elapsed_infection_time(person_id);
    let current_infectiousness = total_rate;

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        if !context.sample_bool(
            ForecastRng,
            current_infectiousness / forecasted_total_infectiousness,
        ) {
            trace!("Person{person_id}: Forecast rejected");

            return None;
        }
    }

    context.get_contact_from_setting(
        person_id,
        current_setting,
        (
            (Alive, true),
            (InfectiousStatus, InfectiousStatusValue::Susceptible),
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
        let rate = self.get_global_property_value(Parameters).unwrap().r_0;
        self.set_person_property(person_id, TimeOfInfection, Some(OrderedFloat(t)));
        self.set_person_property(person_id, InfectiousnessRate, Some(OrderedFloat(rate)));
    }

    fn get_start_of_infection(&self, person_id: PersonId) -> f64 {
        self.get_person_property(person_id, TimeOfInfection)
            .expect("Person doesn't have a TimeOfInfection assigned")
            .0
    }

    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64 {
        let current_time = self.get_current_time();
        let start_of_infection = self.get_start_of_infection(person_id);
        current_time - start_of_infection
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod test {
    use std::path::PathBuf;

    use super::{
        calc_setting_infectiousness_multiplier, get_forecast, max_total_infectiousness_multiplier,
        InfectionContextExt, TimeOfInfection,
    };
    use crate::{
        infectiousness_manager::ALPHA,
        parameters::{Parameters, ParametersValues},
        population_loader::CensusTract,
    };
    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt};
    use ordered_float::OrderedFloat;

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
                    r_0: 1.0,
                    infection_duration: 5.0,
                    report_period: 1.0,
                    synth_population_file: PathBuf::from("."),
                },
            )
            .unwrap();
        context
    }

    #[test]
    fn test_get_start_of_infection() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.set_person_property(p1, TimeOfInfection, Some(OrderedFloat(2.0)));
        assert_eq!(context.get_start_of_infection(p1), 2.0);
    }

    #[test]
    fn test_get_elapsed_infection_time() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.set_person_property(p1, TimeOfInfection, Some(OrderedFloat(1.0)));
        context.add_plan(3.0, move |context| {
            assert_eq!(context.get_elapsed_infection_time(p1), 2.0);
        });
        context.execute();
    }

    #[test]
    fn test_assign_infection_properties() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        context.add_plan(1.0, move |context| {
            context.assign_infection_properties(p1);
            assert_eq!(context.get_start_of_infection(p1), 1.0);
        });
        context.execute();
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        // Add two additional contacts, which should make the factor 2
        context.add_person((CensusTract, 1)).unwrap();
        context.add_person((CensusTract, 1)).unwrap();

        // Number of additional contacts ^ Alpha
        let expected = 2.0_f64.powf(ALPHA);
        assert_eq!(
            calc_setting_infectiousness_multiplier(&context, p1, CensusTract),
            expected
        );
        // For now, max is always just the value of the CensusTract infectiousness
        assert_eq!(max_total_infectiousness_multiplier(&context, p1), expected);
    }

    #[test]
    fn test_forecast() {
        let mut context = setup_context();
        let p1 = context.add_person((CensusTract, 1)).unwrap();
        // Add two additional contacts, which should make the factor 2
        context.add_person((CensusTract, 1)).unwrap();
        context.add_person((CensusTract, 1)).unwrap();

        context.assign_infection_properties(p1);

        let f = get_forecast(&context, p1).expect("Forecast should be returned");
        // The expected rate is 2.0, because intrinsic is 1.0 and there are 2 contacts.
        // TODO<ryl8@cdc>: Check if the times are reasonable
        assert_eq!(f.forecasted_total_infectiousness, 2.0);
    }
}
