use ixa::{
    define_data_plugin, trace, Context, ContextPeopleExt, IxaError, PersonId, PersonProperty,
};
use std::{any::TypeId, cell::RefCell, collections::HashMap};

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

/// A marker trait for identifying a transmission modifier. Only needs to be implemented when the
/// user is specifying a custom transmission modifier function and is usually implemented on unit
/// structs. Each type that implements the trait can invoke the default method implementation.
pub trait TransmissionModifier: std::fmt::Debug + 'static {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

// Since person properties are used as transmission modifier identifiers when registering a
// transmission modifier that depends on a person property, we provide a blanket implementation
// of the transmission modifier trait for all person properties.
impl<T> TransmissionModifier for T where T: PersonProperty + std::fmt::Debug + 'static {}

type TransmissionModifierFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFnAndModifiersUsed = (
    Box<TransmissionAggregatorFn>,
    Vec<&'static dyn TransmissionModifier>,
);

#[derive(Default)]
struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectionStatusValue, HashMap<TypeId, Box<TransmissionModifierFn>>>,
    modifier_aggregator:
        HashMap<InfectionStatusValue, Vec<TransmissionAggregatorFnAndModifiersUsed>>,
    used_yet_aggregation: RefCell<HashMap<InfectionStatusValue, HashMap<TypeId, bool>>>,
}

define_data_plugin!(
    TransmissionModifierPlugin,
    TransmissionModifierContainer,
    TransmissionModifierContainer::default()
);

pub trait ContextTransmissionModifierExt {
    /// Register a generic transmission modifier function for a specific infection status.
    /// Float values returned from the modifier function indicate the relative infectiousness or
    /// susceptibility of the person with respect to the base value of 1.0.
    fn register_transmission_modifier_fn<T: TransmissionModifier, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        transmission_modifier_name: T,
        modifier_fn: F,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static;

    /// Register a transmission modifier that depends solely on the value of one person property.
    /// Internally, this method registers a transmission modifier function that returns the float
    /// value associated the person's property value for the given person property as specified in
    /// the `modifier_key` map. All floats declared in this fashion must be between zero and one.
    ///
    /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
    /// subtracted from 1.0 for modifier effect value.
    #[allow(dead_code)]
    fn store_transmission_modifier_values<T: PersonProperty + std::fmt::Debug + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &[(T::Value, f64)],
    ) -> Result<(), IxaError>
    where
        T::Value: std::hash::Hash + Eq;

    /// Evaluate a transmission modifier function for a given person. Useful when defining a custom
    /// aggregator function that depends on a set of transmission modifiers.
    #[allow(dead_code)]
    fn evaluate_transmission_modifier<T>(
        &self,
        transmission_modifier: T,
        person_id: PersonId,
    ) -> f64
    where
        T: TransmissionModifier;

    /// Register a transmission aggregator function for a specific infection status and set of
    /// transmission modifiers. The aggregator function takes `Context` and `PersonId` and calls
    /// `context.evaluate_transmission_modifier` to get the value of a particular transmission
    /// modifier for the person, aggregating those values as is necessary and returning a float.
    /// The `modifiers_included` vector is used to track which transmission modifiers are considered
    /// in this aggregation function. All transmission modifiers not included in at least one
    /// aggregator function are aggregated using the default aggregator and its assumption of
    /// independence of interventions -- returning a final value by multiplying all transmission
    /// modifier values together. Values returned by custom aggregators are aggregated with other
    /// values with the default aggregator of multiplication.
    #[allow(dead_code)]
    fn register_transmission_modifier_aggregator<F>(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: F,
        modifiers_included: Vec<&'static dyn TransmissionModifier>,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static;

    /// Get the relative intrinsic transmission (infectiousness or susceptibility) for a person
    /// based on their infection status. Queries all registered modifier functions and evaluates
    /// them based on the person's properties. Aggregates according to specified aggregator methods
    /// or defaults to multiplying all modifier values together to return the final value.
    fn get_modified_relative_total_transmission_person(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier_fn<T: TransmissionModifier, F>(
        &mut self,
        infection_status: InfectionStatusValue,
        transmission_modifier_name: T,
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
            trace!("Overwriting existing transmission modifier function for infection status {infection_status:?} and modifier {transmission_modifier_name:?}");
        }

        // Register the transmission modifier typeid among those that we will use for aggregation.
        // This lets us track what has been used in aggregation and what has not as we aggregate.
        self.get_data_container_mut(TransmissionModifierPlugin)
            .used_yet_aggregation
            .borrow_mut()
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), false);
    }

    fn store_transmission_modifier_values<T: PersonProperty + std::fmt::Debug + 'static>(
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

        // Register the transmission modifier for tracking whether we use it when aggregating
        self.get_data_container_mut(TransmissionModifierPlugin)
            .used_yet_aggregation
            .borrow_mut()
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), false);

        Ok(())
    }

    fn evaluate_transmission_modifier<T>(
        &self,
        _transmission_modifier: T,
        person_id: PersonId,
    ) -> f64
    where
        T: TransmissionModifier,
    {
        let infection_status = self.get_person_property(person_id, InfectionStatus);

        // Get the registered transmission modifier fn for this person's infection status
        self.get_data_container(TransmissionModifierPlugin)
            .expect("Register a transmission modifier before evaluating.")
            .transmission_modifier_map
            .get(&infection_status)
            .expect("No transmission modifiers registered for person {person_id:?}'s infection status {infection_status:?}.")
            .get(&TypeId::of::<T>())
            .expect("The transmission modifier function is not registered for infection status {infection_status:?}.")
            // Evaluate the function
            (self, person_id)
    }

    fn register_transmission_modifier_aggregator<F>(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: F,
        modifiers_included: Vec<&'static dyn TransmissionModifier>,
    ) where
        F: Fn(&Context, PersonId) -> f64 + 'static,
    {
        // Box the function to store it in the map
        let boxed_fn = Box::new(agg_function);

        self.get_data_container_mut(TransmissionModifierPlugin)
            .modifier_aggregator
            .entry(infection_status)
            .or_default()
            // Store the boxed function as the next in the vector of aggregator functions
            // This allows multiple aggregator functions to be registered for the same infection status.
            // We use this functionality to enable modularity between aggregator functions and
            // modifiers. This way, a user can register the aggregator functions pertinent to their
            // interventions without needing to globally know about all interventions, which would be
            // the case if we only supported one aggregator function that just replaced the default
            // aggregator.
            .push((boxed_fn, modifiers_included));
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
                // Reset the used_yet_aggregation map for this aggregation call
                for value in self
                    .get_data_container(TransmissionModifierPlugin)
                    // We know that if we have gotten this far, the plugin is initialized
                    .unwrap()
                    .used_yet_aggregation
                    .borrow_mut()
                    .get_mut(&infection_status)
                    // Similarly we know that there are some interventions for the infection status
                    .unwrap()
                    .values_mut()
                {
                    *value = false;
                }

                // Run the custom aggregator functions
                let mut modifier_values = vec![];
                if let Some(aggregators) = transmission_modifier_plugin
                    .modifier_aggregator
                    .get(&infection_status)
                {
                    // Iterate through and apply each aggregator function
                    for (aggregator_fn, modifiers_included) in aggregators {
                        modifier_values.push(aggregator_fn(self, person_id));
                        for &modifier in modifiers_included {
                            // Mark the used transmission modifiers
                            *self
                                .get_data_container(TransmissionModifierPlugin)
                                .unwrap()
                                .used_yet_aggregation
                                .borrow_mut()
                                .get_mut(&infection_status)
                                .unwrap()
                                .get_mut(&modifier.type_id())
                                .unwrap() = true;
                        }
                    }
                }

                // Run the default aggregator on all interventions not yet used in the custom
                // aggregator
                let unused_transmission_modifiers: Vec<TypeId> = transmission_modifier_plugin
                    .used_yet_aggregation
                    .borrow()
                    .get(&infection_status)
                    .unwrap()
                    .iter()
                    .filter_map(|(key, used)| if *used { None } else { Some(*key) })
                    .collect();

                for key in &unused_transmission_modifiers {
                    // Add to the value of the transmission modifier floats for those where a custom
                    // aggregator did not use them.
                    // It's safe to unwrap here because we know the modifier function is set up
                    // if it's in our map of used_yet_aggregation
                    modifier_values
                        .push(transmission_modifier_map.get(key).unwrap()(self, person_id));
                }
                // Multiply all the modifier values together to get the final result
                // (This is the default aggregator/independence assumption)
                // What we are really doing here is assuming independence among transmission modifiers
                // where we don't have a custom aggregator specified.
                modifier_values.iter().product()
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

    use super::TransmissionModifier;

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

    #[derive(Debug)]
    struct CustomTransmissionModifierIdentifier;
    impl TransmissionModifier for CustomTransmissionModifierIdentifier {}

    #[test]
    fn test_register_custom_aggregator() {
        let mut context = setup(0);

        // Register a custom modifier function
        context.register_transmission_modifier_fn(
            InfectionStatusValue::Infectious,
            CustomTransmissionModifierIdentifier,
            |_context: &Context, _person_id: PersonId| 0.42,
        );

        // If the person has mandatory intervention turned on, return that as the transmission
        // modifier, ignoring everything else
        context.register_transmission_modifier_aggregator(
            InfectionStatusValue::Infectious,
            |context, person_id| {
                if context.get_person_property(person_id, MandatoryInterventionStatus)
                    == MandatoryIntervention::Partial
                {
                    context.evaluate_transmission_modifier(MandatoryInterventionStatus, person_id)
                } else {
                    context.evaluate_transmission_modifier(InfectiousnessReductionStatus, person_id)
                        * context
                            .evaluate_transmission_modifier(MandatoryInterventionStatus, person_id)
                }
            },
            vec![&MandatoryInterventionStatus, &InfectiousnessReductionStatus],
        );

        let mandatory_on_person = context
            .add_person((
                (MandatoryInterventionStatus, MandatoryIntervention::Partial),
                (
                    InfectiousnessReductionStatus,
                    Some(InfectiousnessReduction::Partial),
                ),
            ))
            .unwrap();

        context.infect_person(mandatory_on_person, None);

        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(mandatory_on_person),
            INFECTIOUS_PARTIAL * 0.42,
            0.0
        );

        let mandatory_off_person = context
            .add_person((
                (MandatoryInterventionStatus, MandatoryIntervention::NoEffect),
                (
                    InfectiousnessReductionStatus,
                    Some(InfectiousnessReduction::Partial),
                ),
            ))
            .unwrap();

        context.infect_person(mandatory_off_person, None);

        assert_almost_eq!(
            context.get_modified_relative_total_transmission_person(mandatory_off_person),
            INFECTIOUS_PARTIAL * 0.42,
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
