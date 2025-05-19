use ixa::{
    define_data_plugin, trace, Context, ContextPeopleExt, IxaError, PersonId, PersonProperty,
};
use std::{any::TypeId, collections::HashMap};

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

type TransmissionModifierFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;

pub trait TransmissionModifier: std::fmt::Debug + 'static {}

impl<T> TransmissionModifier for T where T: PersonProperty + std::fmt::Debug + 'static {}

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
    ///
    /// `PersonProperty` `TypeId`s are not necessarily called by the modifier function, but each
    /// function must be associated with a `PersonProperty` to ensure unique methods are being stored
    /// when declared by the user to be associated with a particular property
    fn register_transmission_modifier_fn<T: TransmissionModifier + 'static + std::fmt::Debug, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_fn: F,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static;

    /// Register a transmission modifier value tuple set for a specific infection status and person
    /// property.
    /// This supplies a default transmission modifier function that maps person property values to
    /// floats as specified in the modifier key. All floats decalred in this fashion have to be between
    /// zero and one.
    ///
    /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
    /// subtracted from 1.0 for modifier effect value.
    ///
    /// Modifier key is taken as a slice to avoid new object creation through Vec
    #[allow(dead_code)]
    fn store_transmission_modifier_values<T: PersonProperty + 'static + std::fmt::Debug>(
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
    fn register_transmission_modifier_aggregator<F>(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: F,
    ) where
        F: Fn(&Vec<(TypeId, f64)>) -> f64 + 'static;

    /// Get the relative intrinsic transmission (infectiousness or susceptibility) for a person based on their
    /// infection status and current properties based on registered modifiers.
    fn get_modified_relative_total_transmission_person(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier_fn<T: TransmissionModifier, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        transmission_modifier: T,
        modifier_fn: F,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static,
    {
        // Box the function to store it in the map
        let boxed_fn = Box::new(modifier_fn);

        // Insert the boxed function into the transmission modifier map, using entry to handle unititialized keys
        if let Some(_modifier_fxn) = self
            .get_data_container_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), boxed_fn)
        {
            trace!("Overwriting existing transmission modifier function for infection status {infection_status:?} and modifier {transmission_modifier:?}");
        }
    }

    fn store_transmission_modifier_values<T: PersonProperty + 'static + std::fmt::Debug>(
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
            if !(0.0..=1.0).contains(&value) {
                return Err(IxaError::IxaError(
                    "Scalar modifier values stored must be between 0.0 and 1.0. ".to_string()
                        + &format!("Value {value} for {person_property:?}::{key:?} is not."),
                ));
            }

            if let Some(old_value) = modifier_map.insert(key, value) {
                return Err(IxaError::IxaError(
                    "Duplicate values provided in modifier key ".to_string()
                        + &format!("Values {old_value} and {value} were both attempted to be registered to key {person_property:?}::{key:?}"),
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

    fn register_transmission_modifier_aggregator<F>(
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

    fn get_modified_relative_total_transmission_person(&self, person_id: PersonId) -> f64 {
        let infection_status = self.get_person_property(person_id, InfectionStatus);

        if let Some(transmission_modifier_plugin) =
            self.get_data_container(TransmissionModifierPlugin)
        {
            if let Some(transmission_modifier_map) = transmission_modifier_plugin
                .transmission_modifier_map
                .get(&infection_status)
            {
                let mut registered_modifiers = Vec::new();
                for (t, f) in transmission_modifier_map {
                    registered_modifiers.push((*t, f(self, person_id)));
                }

                transmission_modifier_plugin.run_aggregator(infection_status, &registered_modifiers)
            } else {
                // If the infection status is not found in the map, return 1.0
                1.0
            }
        } else {
            // If the plugin is not initialized, return 1.0
            1.0
        }
    }
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

    use crate::infectiousness_manager::{InfectionContextExt, InfectionStatusValue};
    use crate::interventions::transmission_modifier_manager::{
        ContextTransmissionModifierExt, TransmissionModifierPlugin,
    };
    use crate::parameters::{GlobalParams, Params, RateFnType};
    use crate::rate_fns::load_rate_fns;
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

    define_person_property!(Age, usize);

    #[test]
    fn test_register_modifier_values() {
        let mut context = Context::new();
        let relative_effect = 0.8;

        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Susceptible,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        // Add people with different intervention statuses
        let partial_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        let full_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
            .unwrap();

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context
            .get_data_container(TransmissionModifierPlugin)
            .unwrap();
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatusValue::Susceptible)
            .unwrap();

        // Check that the modifier map contains the expected values
        assert_eq!(modifier_map.len(), 1);

        let modifier_fn = modifier_map
            .get(&TypeId::of::<MandatoryInterventionStatus>())
            .unwrap();

        // Check that the modifier function returns the expected value
        assert_almost_eq!(modifier_fn(&context, partial_id), relative_effect, 0.0);
        assert_almost_eq!(modifier_fn(&context, full_id), 0.0, 0.0);
    }

    #[test]
    fn test_register_modifier_value_overwrite() {
        let mut context = Context::new();
        let relative_effect = 0.8;

        // Store initial modifier values to be overwritten
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Susceptible,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, relative_effect - 0.2),
                    (MandatoryIntervention::Full, 0.2),
                ],
            )
            .unwrap();

        // Overwrite previous modifier values
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Susceptible,
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        // Add people with different intervention statuses
        let partial_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        let full_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
            .unwrap();

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context
            .get_data_container(TransmissionModifierPlugin)
            .unwrap();
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatusValue::Susceptible)
            .unwrap();

        // Check that the modifier map contains the expected values
        assert_eq!(modifier_map.len(), 1);

        let modifier_fn = modifier_map
            .get(&TypeId::of::<MandatoryInterventionStatus>())
            .unwrap();

        // Check that the modifier function returns the expected value of the overwritten registration
        // The log should also be checked here to make sure the overwrite is recorded
        assert_almost_eq!(modifier_fn(&context, partial_id), relative_effect, 0.0);
        assert_almost_eq!(modifier_fn(&context, full_id), 0.0, 0.0);
    }

    #[test]
    fn test_register_modifier_value_invalid() {
        let mut context = Context::new();

        // Attempt to store invalid modifier values
        let result = context.store_transmission_modifier_values(
            InfectionStatusValue::Susceptible,
            MandatoryInterventionStatus,
            &[
                (MandatoryIntervention::Partial, 1.2), // Invalid value
                (MandatoryIntervention::Full, 0.0),
            ],
        );

        match result.err() {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Scalar modifier values stored must be between 0.0 and 1.0. Value 1.2 for MandatoryInterventionStatus::Partial is not."
                );
            }
            Some(ue) => panic!(
                "Expected an error that Partial attempts to store an invalid modifier. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, seeded infections with no errors."),
        }
    }

    #[test]
    fn test_register_modifier_value_duplicate() {
        let mut context = Context::new();

        // Attempt to store duplicate modifier values
        let result = context.store_transmission_modifier_values(
            InfectionStatusValue::Susceptible,
            MandatoryInterventionStatus,
            &[
                (MandatoryIntervention::Partial, 0.8),
                (MandatoryIntervention::Partial, 0.5), // Duplicate value
            ],
        );

        match result.err() {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Duplicate values provided in modifier key Values 0.8 and 0.5 were both attempted to be registered to key MandatoryInterventionStatus::Partial"
                );
            }
            Some(ue) => panic!(
                "Expected an error that Partial attempts to store a duplicate modifier. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, seeded infections with no errors."),
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_register_arbitrary_modifier_fn() {
        let mut context = Context::new();
        let aged_42 = context.add_person((Age, 42)).unwrap();

        context.register_transmission_modifier_fn(
            InfectionStatusValue::Susceptible,
            Age,
            |context: &Context, person_id: PersonId| {
                let age = context.get_person_property(person_id, Age);
                age as f64 / 100.0
            },
        );

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context
            .get_data_container(TransmissionModifierPlugin)
            .unwrap();
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatusValue::Susceptible)
            .unwrap();

        let modifier_fn = modifier_map.get(&TypeId::of::<Age>()).unwrap();
        // Check that the modifier function returns the expected value
        assert_almost_eq!(modifier_fn(&context, aged_42), 0.42, 0.0);
    }

    #[test]
    fn test_setup_modifier_values_registration_susceptible() {
        let mut context = setup(0);

        let person_id_partial = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        let person_id_full = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
            .unwrap();
        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id_partial),
            SUSCEPTIBLE_PARTIAL,
            0.0
        );
        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id_full),
            0.0,
            0.0
        );
    }

    #[test]
    fn test_setup_modifier_values_registration_infectious() {
        let mut context = setup(0);

        let infectious_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        context.infect_person(infectious_id, None);
        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(infectious_id),
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

        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    fn test_register_aggregator() {
        let mut context = setup(0);

        context.register_transmission_modifier_aggregator(
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

        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id),
            INFECTIOUS_PARTIAL + INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    // Test that the default aggregator works correctly when person properties change
    fn test_get_modified_relative_total_transmission_person() {
        let mut context = setup(0);

        let person_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        context.infect_person(person_id, None);

        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_person_property(
            person_id,
            InfectiousnessReductionStatus,
            Some(InfectiousnessReduction::Partial),
        );
        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_person_property(person_id, InfectiousnessReductionStatus, None);
        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );
    }
}
