use ixa::{define_data_plugin, Context, ContextPeopleExt, PersonId, PersonProperty};
use std::{any::TypeId, collections::HashMap};

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Alive,
};

type TransmissionModifierFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;

struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectionStatusValue, HashMap<TypeId, Box<TransmissionModifierFn>>>,
    modifier_aggregator: HashMap<InfectionStatusValue, Box<TransmissionAggregatorFn>>,
}

impl TransmissionModifierContainer {
    fn run_aggregator(
        &self,
        infection_status: InfectionStatusValue,
        modifiers: &Vec<(TypeId, f64)>,
    ) -> f64 {
        self.modifier_aggregator
            .get(&infection_status)
            .unwrap_or(&Self::default_aggregator())(modifiers)
    }

    fn default_aggregator() -> Box<TransmissionAggregatorFn> {
        Box::new(|modifiers: &Vec<(TypeId, f64)>| -> f64 {
            let mut aggregate_effects = 1.0;

            for (_, effect) in modifiers {
                aggregate_effects *= effect;
            }

            aggregate_effects
        })
    }
}

define_data_plugin!(
    TransmissionModifierPlugin,
    TransmissionModifierContainer,
    TransmissionModifierContainer {
        transmission_modifier_map: HashMap::new(),
        modifier_aggregator: HashMap::new(),
    }
);

pub trait ContextTransmissionModifierExt {
    /// Register a transmission modifier for a specific infection status and person property.
    /// Modifier key must have specified lifetime to outlive the Box'd `TrasnmissionModifierFn`.
    /// Modifier key is taken as a slice to avoid new object creation through Vec
    fn register_transmission_modifier<T: PersonProperty + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &'static [(T::Value, f64)],
    );

    /// Register a transmission aggregator for a specific infection status.
    /// The aggregator is a function that takes a vector of tuples containing the type ID of the person property
    /// and its corresponding modifier value.
    /// The default aggregator multiplies all the modifier values together independently.
    #[allow(dead_code)]
    fn register_transmission_aggregator(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: Box<TransmissionAggregatorFn>,
    );

    /// Get the relative intrinsic transmission (infectiousness or susceptiblity) for a person based on their
    /// infection status and current properties based on registered modifiers.
    fn get_relative_intrinsic_transmission_person(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier<T: PersonProperty + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &'static [(T::Value, f64)],
    ) {
        let transmission_modifier_map: HashMap<TypeId, Box<TransmissionModifierFn>> =
            HashMap::from([(
                TypeId::of::<T>(),
                Box::new(move |context: &Context, person_id| -> f64 {
                    let property_val = context.get_person_property(person_id, person_property);
                    for item in modifier_key {
                        if property_val == item.0 {
                            return item.1;
                        }
                    }
                    // Return a default 1.0 (no relative change if unregistered)
                    1.0
                }) as Box<dyn Fn(&Context, PersonId) -> f64>,
            )]);

        self.get_data_container_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .extend(transmission_modifier_map);
    }

    fn register_transmission_aggregator(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: Box<TransmissionAggregatorFn>,
    ) {
        let transmission_modifier_container =
            self.get_data_container_mut(TransmissionModifierPlugin);

        transmission_modifier_container
            .modifier_aggregator
            .insert(infection_status, agg_function);
    }

    fn get_relative_intrinsic_transmission_person(&self, person_id: PersonId) -> f64 {
        let infection_status = self.get_person_property(person_id, InfectionStatus);

        let transmission_modifier_plugin =
            self.get_data_container(TransmissionModifierPlugin).unwrap();

        let transmission_modifer_map = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&infection_status)
            .unwrap();

        let mut registered_modifiers = Vec::new();
        for (t, f) in transmission_modifer_map {
            registered_modifiers.push((*t, f(self, person_id)));
        }

        transmission_modifier_plugin.run_aggregator(infection_status, &registered_modifiers)
    }
}

// Initialize the transmission modifier plugin with guaranteed values
pub fn init(context: &mut Context) {
    context.register_transmission_modifier(
        InfectionStatusValue::Susceptible,
        Alive,
        &[(true, 1.0), (false, 0.0)],
    );
    context.register_transmission_modifier(
        InfectionStatusValue::Infectious,
        Alive,
        &[(true, 1.0), (false, 0.0)],
    );
}

#[cfg(test)]
mod test {
    use ixa::{
        define_person_property, define_person_property_with_default, Context,
        ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
    };
    use serde::{Deserialize, Serialize};
    use statrs::assert_almost_eq;
    use std::{any::TypeId, path::PathBuf};

    use crate::infectiousness_manager::{
        evaluate_forecast, get_forecast, infection_attempt, InfectionContextExt, InfectionData,
        InfectionDataValue, InfectionStatusValue,
    };
    use crate::interventions::transmission_modifier_manager::ContextTransmissionModifierExt;
    use crate::parameters::{GlobalParams, ItineraryWriteFnType, Params, RateFnType};
    use crate::rate_fns::{load_rate_fns, InfectiousnessRateExt};
    use crate::settings::{
        create_itinerary, define_setting_type, ContextSettingExt, SettingProperties,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum MandatoryIntervention {
        Partial,
        Full,
        NoEffect,
    }
    define_person_property!(MandatoryInterventionStatus, MandatoryIntervention);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum InfectiousnessReduction {
        Partial,
    }
    define_person_property_with_default!(
        InfectiousnessReductionStatus,
        Option<InfectiousnessReduction>,
        None
    );

    pub const SUSCEPTIBLE_PARTIAL: f64 = 0.8;
    pub const INFECTIOUS_PARTIAL: f64 = 0.5;

    define_setting_type!(HomogeneousMixing);
    fn set_homogenous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = create_itinerary(context, vec![(TypeId::of::<HomogeneousMixing>(), 0)])?;
        context.add_itinerary(person_id, itinerary)
    }

    fn setup(seed: u64, rate: f64) -> Context {
        let mut context = Context::new();
        context.init_random(seed);
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    initial_infections: 1,
                    max_time: 10.0,
                    seed,
                    infectiousness_rate_fn: RateFnType::Constant {
                        rate,
                        duration: 5.0,
                    },
                    symptom_progression_library: None,
                    report_period: 1.0,
                    synth_population_file: PathBuf::from("."),
                    transmission_report_name: None,
                    settings_properties: vec![],
                    itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
                },
            )
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
            .register_setting_type(HomogeneousMixing {}, SettingProperties { alpha: 1.0 })
            .unwrap();

        context.register_transmission_modifier(
            InfectionStatusValue::Susceptible,
            MandatoryInterventionStatus,
            &[
                (MandatoryIntervention::Partial, SUSCEPTIBLE_PARTIAL),
                (MandatoryIntervention::Full, 0.0),
            ],
        );
        context.register_transmission_modifier(
            InfectionStatusValue::Infectious,
            MandatoryInterventionStatus,
            &[
                (MandatoryIntervention::Partial, INFECTIOUS_PARTIAL),
                (MandatoryIntervention::Full, 0.0),
            ],
        );
        context.register_transmission_modifier(
            InfectionStatusValue::Infectious,
            InfectiousnessReductionStatus,
            &[(Some(InfectiousnessReduction::Partial), INFECTIOUS_PARTIAL)],
        );
        context
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_transmission_modifier_registration_susceptible() {
        let mut context = setup(0, 1.0);

        let person_id_partial: ixa::PersonId = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        let person_id_full = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
            .unwrap();
        assert_eq!(
            context.get_relative_intrinsic_transmission_person(person_id_partial),
            SUSCEPTIBLE_PARTIAL
        );
        assert_eq!(
            context.get_relative_intrinsic_transmission_person(person_id_full),
            0.0
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_transmission_modifier_registration_infectious() {
        let mut context = setup(0, 1.0);

        let infectious_id = context
            .add_person((
                (
                    InfectionData,
                    InfectionDataValue::Infectious {
                        infection_time: 0.0,
                        rate_fn_id: context.get_random_rate_fn(),
                        infected_by: None,
                    },
                ),
                (MandatoryInterventionStatus, MandatoryIntervention::Partial),
            ))
            .unwrap();
        assert_eq!(
            context.get_relative_intrinsic_transmission_person(infectious_id),
            INFECTIOUS_PARTIAL
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_get_relative_intrinsic_transmission_person() {
        let mut context = setup(0, 1.0);

        let person_id = context
            .add_person((
                (MandatoryInterventionStatus, MandatoryIntervention::Partial),
                (
                    InfectionData,
                    InfectionDataValue::Infectious {
                        infection_time: 0.0,
                        rate_fn_id: context.get_random_rate_fn(),
                        infected_by: None,
                    },
                ),
            ))
            .unwrap();

        assert_eq!(
            context.get_relative_intrinsic_transmission_person(person_id),
            INFECTIOUS_PARTIAL
        );

        context.set_person_property(
            person_id,
            InfectiousnessReductionStatus,
            Some(InfectiousnessReduction::Partial),
        );
        assert_eq!(
            context.get_relative_intrinsic_transmission_person(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_rejection_sample_infection_attempt_intervention() {
        let n = 1000;
        let mut count = 0;
        for i in 0..n {
            let mut context = setup(i, 1.0);
            let person = context
                .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
                .unwrap();
            if infection_attempt(&context, person) {
                count += 1;
            }
        }
        assert_almost_eq!(count as f64 / n as f64, SUSCEPTIBLE_PARTIAL, 0.01);
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_rejection_sample_forecast_intervention() {
        let n = 10_000;
        let mut count = 0;
        for seed in 0..n {
            let mut context = setup(seed, 3.0);

            let infector = context
                .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
                .unwrap();
            let target = context
                .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
                .unwrap();

            set_homogenous_mixing_itinerary(&mut context, infector).unwrap();
            set_homogenous_mixing_itinerary(&mut context, target).unwrap();

            context.infect_person(infector, None);
            let forecast = get_forecast(&context, infector).unwrap();
            if evaluate_forecast(
                &mut context,
                infector,
                forecast.forecasted_total_infectiousness,
            ) {
                count += 1;
            }
        }
        assert_almost_eq!(count as f64 / n as f64, INFECTIOUS_PARTIAL, 0.01);
    }
}
