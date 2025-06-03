use ixa::{
    define_data_plugin, trace, Context, ContextPeopleExt, IxaError, PersonId, PersonProperty,
};
use std::{any::TypeId, collections::HashMap};

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

/// Defines a transmission modifier that is used to modify the transmissiveness or susceptibility
/// of a person based on their infection status.
// We require `Debug` for easy logging of the trait so the user can see what is happening.
pub trait TransmissionModifier: std::fmt::Debug + 'static {
    /// Return the relative potential for infection (transmissiveness or susceptibility) for a person
    /// based on their infection status.
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64;

    /// For debugging purposes. The name of the transmission modifier. The default implementation
    /// returns the `Debug` representation of the transmission modifier struct on which this trait
    /// is implemented.
    fn get_name(&self) -> String {
        format!("{self:?}")
    }
}

// A type alias for the type of the transmission modifiers specified via a hashmap of person
// property values and floats -- i.e., `modifier_key: &[(T::Value, f64)]`
type PersonPropertyModifier<T> = (
    T,
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    HashMap<<T as PersonProperty>::Value, f64>,
);

impl<T> TransmissionModifier for PersonPropertyModifier<T>
where
    // All person properties implement `Debug` and are static
    T: PersonProperty + std::fmt::Debug + 'static,
    // For now, this limits us to person property values that are not floats for use in the
    // transmisison modifier map convienience method.
    T::Value: std::hash::Hash + Eq,
{
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
        let (person_property, modifier_map) = self;
        let property_val = context.get_person_property(person_id, *person_property);
        // Return the corresponding value from the map, or 1.0 if not found
        *modifier_map.get(&property_val).unwrap_or(&1.0)
    }
    fn get_name(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[derive(Default)]
struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectionStatusValue, HashMap<TypeId, Box<dyn TransmissionModifier>>>,
}

define_data_plugin!(
    TransmissionModifierPlugin,
    TransmissionModifierContainer,
    TransmissionModifierContainer::default()
);

pub trait ContextTransmissionModifierExt {
    /// Register a generic transmission modifier for a specific infection status.
    fn register_transmission_modifier_fn<T: TransmissionModifier>(
        &mut self,
        infection_status: InfectionStatusValue,
        transmission_modifier: T,
    );

    /// Register a transmission modifier that depends solely on the value of one person property.
    /// The function accepts a relative transmission potential key, which is a slice of tuples that
    /// associate values of a specified person property with the relativ etransmission potential of
    /// a person with that property value. All floats declared in this fashion must be between zero
    /// and one and represent the proportion of infectiousness or susceptiblity remaining if a
    /// modifier is active.
    ///
    /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
    /// subtracted from 1.0 for effect on relative transmission potential.
    ///
    /// Internally, this method registers a transmission modifier function that returns the float
    /// value associated the person's property value in the
    /// `relative_transmission_potential_multipliers` key.
    #[allow(dead_code)]
    fn store_transmission_modifier_values<T: PersonProperty + std::fmt::Debug + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        relative_transmission_potential_multipliers: &[(T::Value, f64)],
    ) -> Result<(), IxaError>
    where
        T::Value: std::hash::Hash + Eq;

    /// Get the relative potential for infection (infectiousness or susceptibility) for a person
    /// based on their infection status based on all registered modifiers. Queries all registered
    /// modifier functions and evaluates them based on the person's properties. Multiplies them
    /// together to get the total relative transmission modifier for the person.
    /// Returns 1.0 if no modifiers are registered for the person's infection status.
    fn get_relative_total_transmission(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier_fn<T: TransmissionModifier>(
        &mut self,
        infection_status: InfectionStatusValue,
        transmission_modifier: T,
    ) {
        // Box the transmission modifier to store it in the map
        // Transmission modifiers must implement debug so that we can more easily log their addition
        let name = transmission_modifier.get_name();
        let boxed_transmission_modifier = Box::new(transmission_modifier);

        // Insert the boxed function into the transmission modifier map, using entry to handle unititialized keys
        if let Some(_modifier_fxn) = self
            .get_data_container_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), boxed_transmission_modifier)
        {
            trace!("Overwriting existing transmission modifier function for infection status {infection_status:?} and transmission modifier {name}");
        }
    }

    fn store_transmission_modifier_values<T: PersonProperty + std::fmt::Debug + 'static>(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        relative_transmission_potential_multipliers: &[(T::Value, f64)],
    ) -> Result<(), IxaError>
    where
        T::Value: std::hash::Hash + Eq,
    {
        // Convert modifiers to HashMap
        let mut modifier_map = HashMap::new();
        for &(key, value) in relative_transmission_potential_multipliers {
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
        self.register_transmission_modifier_fn(infection_status, (person_property, modifier_map));
        Ok(())
    }

    fn get_relative_total_transmission(&self, person_id: PersonId) -> f64 {
        let infection_status = self.get_person_property(person_id, InfectionStatus);

        if let Some(transmission_modifier_plugin) =
            self.get_data_container(TransmissionModifierPlugin)
        {
            if let Some(transmission_modifier_map) = transmission_modifier_plugin
                .transmission_modifier_map
                .get(&infection_status)
            {
                // Calculate the relative modifier for each registered function and multiply them
                // together to get the total relative transmission modifier for the person
                transmission_modifier_map
                    .iter()
                    .scan(1.0, |state, (_, tm)| {
                        Some(*state * tm.get_relative_transmission(self, person_id))
                    })
                    .product()
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

    use super::{
        ContextTransmissionModifierExt, PersonPropertyModifier, TransmissionModifier,
        TransmissionModifierPlugin,
    };
    use crate::{
        infectiousness_manager::{InfectionContextExt, InfectionStatusValue},
        parameters::{GlobalParams, Params, RateFnType},
        rate_fns::load_rate_fns,
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
    pub enum InfectiousnessProportion {
        None,
        Partial,
    }
    define_person_property_with_default!(
        InfectiousnessProportionStatus,
        InfectiousnessProportion,
        InfectiousnessProportion::None
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
                    // For those tests that need infectious people, we add them manually.
                    initial_incidence: 0.0,
                    initial_recovered: 0.0,
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
                    isolation_policy_parameters: HashMap::new(),
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
                InfectiousnessProportionStatus,
                &[(InfectiousnessProportion::Partial, INFECTIOUS_PARTIAL)],
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
            .get(&TypeId::of::<
                PersonPropertyModifier<MandatoryInterventionStatus>,
            >())
            .unwrap();

        // Check that the modifier function returns the expected value
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, partial_id),
            relative_effect,
            0.0
        );
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, full_id),
            0.0,
            0.0
        );
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
            .get(&TypeId::of::<
                PersonPropertyModifier<MandatoryInterventionStatus>,
            >())
            .unwrap();

        // Check that the modifier function returns the expected value of the overwritten registration
        // The log should also be checked here to make sure the overwrite is recorded
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, partial_id),
            relative_effect,
            0.0
        );
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, full_id),
            0.0,
            0.0
        );
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

    #[derive(Debug)]
    struct AgeModifier {
        age_multiplier: f64,
    }

    impl TransmissionModifier for AgeModifier {
        #[allow(clippy::cast_precision_loss)]
        fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
            let age = context.get_person_property(person_id, Age);
            age as f64 * self.age_multiplier
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_register_arbitrary_modifier_fn() {
        let mut context = Context::new();
        let aged_42 = context.add_person((Age, 42)).unwrap();

        context.register_transmission_modifier_fn(
            InfectionStatusValue::Susceptible,
            AgeModifier {
                age_multiplier: 0.01,
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

        let modifier_fn = modifier_map.get(&TypeId::of::<AgeModifier>()).unwrap();
        // Check that the modifier function returns the expected value
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, aged_42),
            0.42,
            0.0
        );
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
            context.get_relative_total_transmission(person_id_partial),
            SUSCEPTIBLE_PARTIAL,
            0.0
        );
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id_full),
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
        context.infect_person(infectious_id, None, None, None);
        assert_almost_eq!(
            context.get_relative_total_transmission(infectious_id),
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
                    InfectiousnessProportionStatus,
                    InfectiousnessProportion::Partial,
                ),
            ))
            .unwrap();

        context.infect_person(person_id, None, None, None);

        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    // Test that the default aggregator works correctly when person properties change
    fn test_get_relative_total_transmission() {
        let mut context = setup(0);

        let person_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
            .unwrap();
        context.infect_person(person_id, None, None, None);

        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_person_property(
            person_id,
            InfectiousnessProportionStatus,
            InfectiousnessProportion::Partial,
        );
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_person_property(
            person_id,
            InfectiousnessProportionStatus,
            InfectiousnessProportion::None,
        );
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );
    }
}
