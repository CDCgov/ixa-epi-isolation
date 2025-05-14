use ixa::{define_data_plugin, Context, ContextPeopleExt, IxaError, PersonId, PersonProperty};
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
            .map_or_else(|| default_aggregator(modifiers), |agg| agg(modifiers))
    }
}

fn default_aggregator(modifiers: &[(TypeId, f64)]) -> f64 {
    modifiers.iter().map(|&(_, f)| f).product()
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
    /// Register a transmission modifier function for a specific infection status and person property.
    /// All float values returned should return the relative infectiousness or susceptibility
    /// of the person with respect to the base value of 1.0.
    fn register_transmission_modifier_fn<T: PersonProperty + 'static, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_fn: F,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static;

    /// Register a transmission modifier value tuple set for a specific infection status and person
    /// property.
    /// This supplies a default transmission modifier function that maps person property values to
    /// floats as specified in the modifier key.
    ///
    /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
    /// subtracted from 1.0 for modifier effect value.
    ///
    /// Modifier key is taken as a slice to avoid new object creation through Vec
    fn store_transmission_modifier_values<T: PersonProperty + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &[(T::Value, f64)],
    ) -> Result<(), IxaError>
    where
        T::Value: std::hash::Hash + Eq;

    /// Register a transmission aggregator for a specific infection status.
    /// The aggregator is a function that takes a vector of tuples containing the type ID of the person property
    /// and its corresponding modifier value.
    /// The default aggregator multiplies all the modifier values together independently.
    #[allow(dead_code)]
    fn register_transmission_aggregator<F>(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: F,
    ) where
        F: Fn(&Vec<(TypeId, f64)>) -> f64 + 'static;

    /// Get the relative intrinsic transmission (infectiousness or susceptibility) for a person based on their
    /// infection status and current properties based on registered modifiers.
    fn get_relative_intrinsic_transmission_person(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier_fn<T: PersonProperty + 'static, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        _person_property: T,
        modifier_fn: F,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static,
    {
        // Box the function to store it in the map
        let boxed_fn =
            Box::new(move |context: &Context, person_id: PersonId| modifier_fn(context, person_id))
                as Box<dyn Fn(&Context, PersonId) -> f64>;

        // Insert the boxed function into the transmission modifier map, using entry to handle unititialized keys
        self.get_data_container_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), boxed_fn);
    }

    fn store_transmission_modifier_values<T: PersonProperty + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &[(T::Value, f64)],
    ) -> Result<(), IxaError>
    where
        T::Value: std::hash::Hash + Eq,
    {
        // Convert modifiers to HashMap
        let mut modifier_map = HashMap::new();
        for &(key, value) in modifier_key {
            if let Some(value) = modifier_map.insert(key, value) {
                return Err(IxaError::IxaError(
                    "Duplicate values provided in modifier key ".to_string()
                        + &format!("Value {value} was replaced for key {key:?}"),
                ));
            }
        }

        // Register a default function to simply map floats with T::Values
        self.register_transmission_modifier_fn(
            infection_status,
            person_property,
            move |context: &Context, person_id: PersonId| -> f64 {
                let property_val = context.get_person_property(person_id, person_property);
                // Return the corresponding value from the map, or 1.0 if not found
                *modifier_map.get(&property_val).unwrap_or(&1.0)
            },
        );
        Ok(())
    }

    fn register_transmission_aggregator<F>(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: F,
    ) where
        F: Fn(&Vec<(TypeId, f64)>) -> f64 + 'static,
    {
        // Box the function to store it in the map
        let boxed_fn = Box::new(agg_function);

        self.get_data_container_mut(TransmissionModifierPlugin)
            .modifier_aggregator
            .insert(infection_status, boxed_fn);
    }

    fn get_relative_intrinsic_transmission_person(&self, person_id: PersonId) -> f64 {
        let infection_status = self.get_person_property(person_id, InfectionStatus);

        if let Some(transmission_modifier_plugin) =
            self.get_data_container(TransmissionModifierPlugin)
        {
            let transmission_modifier_map = transmission_modifier_plugin
                .transmission_modifier_map
                .get(&infection_status)
                .unwrap();

            let mut registered_modifiers = Vec::new();
            for (t, f) in transmission_modifier_map {
                registered_modifiers.push((*t, f(self, person_id)));
            }

            transmission_modifier_plugin.run_aggregator(infection_status, &registered_modifiers)
        } else {
            // If the plugin is not initialized, return 1.0
            1.0
        }
    }
}

// Initialize the transmission modifier plugin with guaranteed values
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.store_transmission_modifier_values(
        InfectionStatusValue::Susceptible,
        Alive,
        &[(true, 1.0), (false, 0.0)],
    )?;
    context.store_transmission_modifier_values(
        InfectionStatusValue::Infectious,
        Alive,
        &[(true, 1.0), (false, 0.0)],
    )?;
    Ok(())
}

#[cfg(test)]
mod test {
    use ixa::{
        define_person_property, define_person_property_with_default, Context,
        ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
    };
    use serde::{Deserialize, Serialize};
    use statrs::assert_almost_eq;
    use std::{collections::HashMap, path::PathBuf};

    use crate::infectiousness_manager::{
        evaluate_forecast, get_forecast, infection_attempt, InfectionContextExt, InfectionData,
        InfectionDataValue, InfectionStatusValue,
    };
    use crate::interventions::transmission_modifier_manager::{
        ContextTransmissionModifierExt, TransmissionModifierPlugin,
    };
    use crate::parameters::{GlobalParams, ItinerarySpecificationType, Params, RateFnType};
    use crate::rate_fns::{load_rate_fns, InfectiousnessRateExt};
    use crate::settings::{
        define_setting_type, ContextSettingExt, ItineraryEntry, SettingId, SettingProperties,
    };
    use std::any::TypeId;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum MandatoryIntervention {
        Partial,
        Full,
        NoEffect,
    }
    define_person_property!(MandatoryInterventionStatus, MandatoryIntervention);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
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
    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = vec![ItineraryEntry::new(
            &SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup(seed: u64) -> Context {
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
                        rate: 1.0,
                        duration: 10.0,
                    },
                    symptom_progression_library: None,
                    report_period: 1.0,
                    synth_population_file: PathBuf::from("."),
                    transmission_report_name: None,
                    settings_properties: HashMap::new(),
                },
            )
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
            .register_setting_type(
                HomogeneousMixing,
                SettingProperties {
                    alpha: 1.0,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 1.0,
                    }),
                },
            )
            .unwrap();

        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Susceptible,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, SUSCEPTIBLE_PARTIAL),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, INFECTIOUS_PARTIAL),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                InfectiousnessReductionStatus,
                &[(Some(InfectiousnessReduction::Partial), INFECTIOUS_PARTIAL)],
            )
            .unwrap();
        context
    }

    #[test]
    fn test_transmission_modifier_values_registration_susceptible() {
        let mut context = setup(0);

        let person_id_partial = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        let person_id_full = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
            .unwrap();
        assert_almost_eq!(
            context.get_relative_intrinsic_transmission_person(person_id_partial),
            SUSCEPTIBLE_PARTIAL,
            0.0
        );
        assert_almost_eq!(
            context.get_relative_intrinsic_transmission_person(person_id_full),
            0.0,
            0.0
        );
    }

    #[test]
    fn test_transmission_modifier_values_registration_infectious() {
        let mut context = setup(0);

        let infectious_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        context.infect_person(infectious_id, None);
        assert_almost_eq!(
            context.get_relative_intrinsic_transmission_person(infectious_id),
            INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    fn test_default_aggregator_from_container() {
        let mut context = setup(0);

        let person_id = context
            .add_person((
                (MandatoryInterventionStatus, MandatoryIntervention::Partial),
                (
                    InfectiousnessReductionStatus,
                    Some(InfectiousnessReduction::Partial),
                ),
            ))
            .unwrap();

        context.infect_person(person_id, None);

        let transmission_modifier_plugin = context
            .get_data_container(TransmissionModifierPlugin)
            .unwrap();

        let transmission_modifier_map = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&InfectionStatusValue::Infectious)
            .unwrap();

        let mut registered_modifiers = Vec::new();
        for (t, f) in transmission_modifier_map {
            registered_modifiers.push((*t, f(&context, person_id)));
        }

        assert_almost_eq!(
            transmission_modifier_plugin
                .run_aggregator(InfectionStatusValue::Infectious, &registered_modifiers),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    fn test_register_aggregator() {
        let mut context = setup(0);

        context.register_transmission_aggregator(
            InfectionStatusValue::Infectious,
            |modifiers: &Vec<(TypeId, f64)>| {
                // Custom aggregator that sums the values
                modifiers.iter().map(|&(_, f)| f).sum()
            },
        );

        let person_id = context
            .add_person((
                (MandatoryInterventionStatus, MandatoryIntervention::Partial),
                (
                    InfectiousnessReductionStatus,
                    Some(InfectiousnessReduction::Partial),
                ),
            ))
            .unwrap();

        context.infect_person(person_id, None);

        let transmission_modifier_plugin = context
            .get_data_container(TransmissionModifierPlugin)
            .unwrap();

        let transmission_modifier_map = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&InfectionStatusValue::Infectious)
            .unwrap();

        let mut registered_modifiers = Vec::new();
        for (t, f) in transmission_modifier_map {
            registered_modifiers.push((*t, f(&context, person_id)));
        }

        assert_almost_eq!(
            transmission_modifier_plugin
                .run_aggregator(InfectionStatusValue::Infectious, &registered_modifiers),
            INFECTIOUS_PARTIAL + INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    fn test_get_relative_intrinsic_transmission_person() {
        let mut context = setup(0);

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

        assert_almost_eq!(
            context.get_relative_intrinsic_transmission_person(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_person_property(
            person_id,
            InfectiousnessReductionStatus,
            Some(InfectiousnessReduction::Partial),
        );
        assert_almost_eq!(
            context.get_relative_intrinsic_transmission_person(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_rejection_sample_infection_attempt_intervention() {
        let n = 1000;
        let mut count = 0;
        for i in 0..n {
            let mut context = setup(i);
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
            let mut context = setup(seed);

            let infector = context
                .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
                .unwrap();
            let target = context
                .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
                .unwrap();

            set_homogeneous_mixing_itinerary(&mut context, infector).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, target).unwrap();

            context.infect_person(infector, None);
            // Will return None if forecast is greater than infection duration
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
