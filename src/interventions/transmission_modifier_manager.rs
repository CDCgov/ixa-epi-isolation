use ixa::{define_data_plugin, Context, ContextPeopleExt, PersonId, PersonProperty};
use std::{any::TypeId, collections::HashMap};

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

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
    fn register_transmission_modifier<
        T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash,
    >(
        &mut self,
        infection_status: InfectionStatusValue,
        person_property: T,
        modifier_key: &'static [(T::Value, f64)],
    );

    /// Register a transmission aggregator for a specific infection status.
    /// The aggregator is a function that takes a vector of tuples containing the type ID of the person property
    /// and its corresponding modifier value.
    /// The default aggregator multiplies all the modifier values together independently.
    fn register_transmission_aggregator(
        &mut self,
        infection_status: InfectionStatusValue,
        agg_function: Box<TransmissionAggregatorFn>,
    );

    /// Get the relative intrinsic transmission (infectiousness or susceptiblity) for a person based on their
    /// infection status and current properties based on registered modifiers.
    fn get_relative_intrinsic_transmission_person(&self, person_id: PersonId) -> f64;

    /// Get the relative transmission for a contact attempt between two people.
    /// This is the independent product of the relative intrinsic transmission for both people.
    fn get_relative_transmission_infection_attempt(
        &self,
        transmitter_id: PersonId,
        contact_id: PersonId,
    ) -> f64;
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
            .insert(infection_status, transmission_modifier_map);
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

        let mut registered_modifiers = Vec::new();
        let transmission_modifier_plugin =
            self.get_data_container(TransmissionModifierPlugin).unwrap();

        let transmission_modifer_map = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&infection_status)
            .unwrap();

        for (t, f) in transmission_modifer_map {
            registered_modifiers.push((*t, f(self, person_id)));
        }

        transmission_modifier_plugin.run_aggregator(infection_status, &registered_modifiers)
    }

    fn get_relative_transmission_infection_attempt(
        &self,
        transmitter_id: PersonId,
        contact_id: PersonId,
    ) -> f64 {
        self.get_relative_intrinsic_transmission_person(transmitter_id)
            * self.get_relative_intrinsic_transmission_person(contact_id)
    }
}
