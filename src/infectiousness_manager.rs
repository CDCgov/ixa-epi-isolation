use ixa::{
    define_derived_property, define_person_property_with_default, define_rng, trace, Context,
    ContextPeopleExt, ContextRandomExt, PersonId, PluginContext,
};
use serde::Serialize;
use statrs::distribution::Exp;

use crate::{
    interventions::ContextTransmissionModifierExt,
    rate_fns::{InfectiousnessRateExt, InfectiousnessRateFn, ScaledRateFn},
    settings::ContextSettingExt,
};

#[derive(Serialize, PartialEq, Debug, Clone, Copy)]
pub enum InfectionDataValue {
    Susceptible,
    Infectious {
        infection_time: f64,
        infected_by: Option<PersonId>,
        infection_setting_type: Option<&'static str>,
        infection_setting_id: Option<usize>,
    },
    Recovered {
        infection_time: f64,
        recovery_time: f64,
    },
}

#[derive(Serialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
pub enum InfectionStatusValue {
    Susceptible,
    Infectious,
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
        InfectionDataValue::Infectious { .. } => InfectionStatusValue::Infectious,
        InfectionDataValue::Recovered { .. } => InfectionStatusValue::Recovered,
    }
);

/// Calculate the scaling factor that accounts for the total infectiousness
/// for a person, given factors related to their environment, such as the number of people
/// they come in contact with or how close they are.
/// This is used to scale the intrinsic infectiousness function of that person.
/// All modifiers of the infector's intrinsic infecitousness are aggregated and returned
/// as a single float to multiply by the base total infectiousness.
/// This assumes that transmission modifiers of total infectiousness are independent of
/// the setting type and are linear
pub fn calc_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    let relative_transmission_potential = context.get_relative_total_transmission(person_id);
    relative_transmission_potential
        * context.calculate_total_infectiousness_multiplier_for_person(person_id)
}

/// Calculate the maximum possible scaling factor for total infectiousness
/// for a person, given information we know at the time of a forecast.
/// The modifier used for intrinsic infectiousness is ignored because all modifiers must
/// be less than or equal to one.
pub fn max_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    context.calculate_total_infectiousness_multiplier_for_person(person_id)
}

define_rng!(ForecastRng);

// Infection attempt function for a context and given `PersonId`
pub fn infection_attempt(context: &mut Context, person_id: PersonId) -> Option<PersonId> {
    if let Some(setting) = context.sample_setting(person_id) {
        let next_contact = context
            .sample_from_setting_with_exclusion(person_id, setting)
            .unwrap()?;
        match context.get_person_property(next_contact, InfectionStatus) {
            InfectionStatusValue::Susceptible => {
                if context.sample_bool(
                    ForecastRng,
                    context.get_relative_total_transmission(next_contact),
                ) {
                    trace!(
                        "Infection attempt successful. Person {}, setting type {} {}, infecting {}",
                        person_id,
                        setting.get_category_id(),
                        setting.id(),
                        next_contact
                    );
                    context.infect_person(
                        next_contact,
                        Some(person_id),
                        Some(setting.get_category_id()),
                        Some(setting.id()),
                    );
                    Some(next_contact)
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

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
        // 1e-10 is a small enough tolerance for floating point comparison.
        (current_infectiousness <= forecasted_total_infectiousness + 1e-10),
        "Person {person_id}: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: {current_infectiousness}, Forecasted: {forecasted_total_infectiousness}"
    );

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        if !context.sample_bool(
            ForecastRng,
            current_infectiousness / forecasted_total_infectiousness,
        ) {
            trace!("Person {person_id}: Forecast rejected");

            return false;
        }
    }

    true
}

pub trait InfectionContextExt: PluginContext + ContextPeopleExt {
    // This function should be called from the main loop whenever
    // someone is first infected. It assigns all their properties needed to
    // calculate intrinsic infectiousness
    fn infect_person(
        &mut self,
        target_id: PersonId,
        source_id: Option<PersonId>,
        setting_type: Option<&'static str>,
        setting_id: Option<usize>,
    ) {
        let infection_time = self.get_current_time();
        trace!("Person {target_id}: Infected at {infection_time}");
        self.set_person_property(
            target_id,
            InfectionData,
            InfectionDataValue::Infectious {
                infection_time,
                infected_by: source_id,
                infection_setting_type: setting_type,
                infection_setting_id: setting_id,
            },
        );
    }
    fn recover_person(&mut self, person_id: PersonId) {
        let recovery_time = self.get_current_time();
        let InfectionDataValue::Infectious { infection_time, .. } =
            self.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infectious")
        };
        self.set_person_property(
            person_id,
            InfectionData,
            InfectionDataValue::Recovered {
                recovery_time,
                infection_time,
            },
        );
    }
    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64 {
        let InfectionDataValue::Infectious { infection_time, .. } =
            self.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infectious")
        };
        self.get_current_time() - infection_time
    }
}
impl InfectionContextExt for Context {}

#[cfg(test)]
mod test {
    use serde::{Deserialize, Serialize};
    use statrs::assert_almost_eq;

    use super::{
        evaluate_forecast, get_forecast, infection_attempt, max_total_infectiousness_multiplier,
        InfectionContextExt,
    };
    use crate::{
        define_setting_category,
        infectiousness_manager::{
            InfectionData, InfectionDataValue, InfectionStatus, InfectionStatusValue,
        },
        interventions::ContextTransmissionModifierExt,
        parameters::{GlobalParams, ItinerarySpecificationType, Params},
        rate_fns::{load_rate_fns, InfectiousnessRateExt},
        settings::{ContextSettingExt, ItineraryEntry, SettingId, SettingProperties},
    };
    use ixa::{
        define_person_property, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, IxaError, PersonId,
    };

    define_setting_category!(HomogeneousMixing);

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = vec![ItineraryEntry::new(
            SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    // For those tests that need infectious people, we add them manually.
                    max_time: 10.0,
                    ..Default::default()
                },
            )
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
            .register_setting_category(
                &HomogeneousMixing,
                SettingProperties {
                    alpha: 1.0,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 1.0,
                    }),
                },
            )
            .unwrap();

        context
    }

    #[test]
    fn test_infect_person() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None, None);
        });
        context.execute();
        let InfectionDataValue::Infectious { infection_time, .. } =
            context.get_person_property(p1, InfectionData)
        else {
            panic!("Person {p1} is not infectious")
        };
        assert_almost_eq!(infection_time, 2.0, 0.0);
        context.get_person_rate_fn(p1);
    }

    #[test]
    fn test_recover_person() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None, None);
        });
        context.add_plan(3.0, move |context| {
            context.recover_person(p1);
        });
        context.execute();
        let InfectionDataValue::Recovered {
            infection_time,
            recovery_time,
        } = context.get_person_property(p1, InfectionData)
        else {
            panic!("Person {p1} is not recovered")
        };
        assert_almost_eq!(infection_time, 2.0, 0.0);
        assert_almost_eq!(recovery_time, 3.0, 0.0);
    }

    #[test]
    fn test_get_elapsed_infection_time() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None, None);
        });
        // Run the simulation until time 3.0 at which the point the individual should have been
        // infected for 1.0 time units.
        context.add_plan(3.0, ixa::Context::shutdown);
        context.execute();
        // Check infection time
        assert_almost_eq!(context.get_elapsed_infection_time(p1), 1.0, 0.0);
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();

        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p1), 0.0, 0.0);
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier_with_contact() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        let p2 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p1), 1.0, 0.0);
        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p2), 1.0, 0.0);
    }

    #[test]
    /// Test has potential to stochastically fail if exponential draw is longer than infectious duration
    fn test_forecast() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        // Add two additional contacts, which should make the factor 2
        let p2 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();
        let p3 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p3).unwrap();

        context.infect_person(p1, None, None, None);

        let f = get_forecast(&context, p1).expect("Forecast should be returned");
        // The expected rate is 2.0, because intrinsic is 1.0 and there are 2 contacts.
        assert_almost_eq!(f.forecasted_total_infectiousness, 2.0, 0.0);
    }

    #[test]
    #[should_panic = "Person 0: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: 1, Forecasted: 0.9"]
    fn test_assert_evaluate_fails_when_forecast_smaller() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        context.infect_person(p1, None, None, None);
        // We need to add another person so that our total infectiousness is 1.
        let p2 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        let invalid_forecast = 1.0 - 0.1;
        evaluate_forecast(&mut context, p1, invalid_forecast);
    }

    #[test]
    fn test_evaluate_still_succeeds_when_forecast_slightly_bigger() {
        let mut context = setup_context();
        let p1 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        context.infect_person(p1, None, None, None);
        let p2 = context.add_person(()).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        let still_valid_forecast = 1.0 - 9e-11;
        assert!(evaluate_forecast(&mut context, p1, still_valid_forecast));
    }

    #[test]
    fn test_infected_options() {
        let mut context = setup_context();
        let index = context.add_person(()).unwrap();
        let contact = context.add_person(()).unwrap();

        context.infect_person(contact, Some(index), Some("Home"), Some(0));
        context.execute();

        assert_eq!(
            context.get_person_property(contact, InfectionStatus),
            InfectionStatusValue::Infectious
        );

        let InfectionDataValue::Infectious {
            infected_by,
            infection_setting_type,
            infection_setting_id,
            ..
        } = context.get_person_property(contact, InfectionData)
        else {
            panic!("Person {contact} is not infectious")
        };

        assert_eq!(infected_by.unwrap(), index);
        assert_eq!(infection_setting_type.unwrap(), "Home");
        assert_eq!(infection_setting_id.unwrap(), 0);
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
    pub enum MandatoryIntervention {
        NoEffect,
        Partial,
        Full,
    }

    define_person_property!(MandatoryInterventionStatus, MandatoryIntervention);

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_rejection_sample_infection_attempt_intervention() {
        let n = 1000;
        let mut count = 0;
        let mut context = setup_context();
        let relative_effect = 0.8;

        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Susceptible,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::NoEffect, 1.0),
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        let contact = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        set_homogeneous_mixing_itinerary(&mut context, contact).unwrap();

        let source = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();
        set_homogeneous_mixing_itinerary(&mut context, source).unwrap();

        for _ in 0..n {
            if infection_attempt(&mut context, source).is_some() {
                count += 1;
                context.set_person_property(
                    contact,
                    InfectionData,
                    InfectionDataValue::Susceptible,
                );
            }
        }
        assert_almost_eq!(count as f64 / n as f64, relative_effect, 0.01);
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_rejection_sample_forecast_intervention() {
        let n = 5_000;
        let mut count = 0;
        let relative_effect = 0.8;

        let mut context = setup_context();

        let p1 = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        let p2 = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();
        let p3 = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p3).unwrap();

        context.infect_person(p1, None, None, None);
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::NoEffect, 1.0),
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        for _ in 0..n {
            // Will return None if forecast is greater than infection duration, running 10_000 times encounters unwrap on None
            let f = get_forecast(&context, p1).expect("Forecast should be returned");
            if evaluate_forecast(&mut context, p1, f.forecasted_total_infectiousness) {
                count += 1;
            }
        }
        assert_almost_eq!(count as f64 / n as f64, relative_effect, 0.01);
    }
}
