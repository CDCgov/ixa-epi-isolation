use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{define_data_plugin, Context, ContextPeopleExt, PersonId, PersonProperty};
use std::{any::TypeId, boxed::Box, collections::HashMap};

type TransmissionModifierFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;

struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectiousStatusType, HashMap<TypeId, Box<TransmissionModifierFn>>>,
    modifier_aggregator: HashMap<InfectiousStatusType, Box<TransmissionAggregatorFn>>,
}

impl TransmissionModifierContainer {
    fn run_aggregator(
        &self,
        infectious_status: InfectiousStatusType,
        modifiers: &Vec<(TypeId, f64)>,
    ) -> f64 {
        self.modifier_aggregator
            .get(&infectious_status)
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
    fn register_transmission_modifier<
        T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash,
    >(
        &mut self,
        infectious_status: InfectiousStatusType,
        person_property: T,
        modifier_key: &Vec<(T::Value, f64)>,
    );
    fn register_transmission_aggregator(
        &mut self,
        infectious_status: InfectiousStatusType,
        agg_function: Box<TransmissionAggregatorFn>,
    );
    fn compute_relative_transmission(&mut self, person_id: PersonId) -> f64;
    fn query_infection_modifers(&mut self, transmitter_id: PersonId, contact_id: PersonId) -> f64;
}

fn default_intervention(modifiers: &Vec<(T::Value, f64)>) -> Box<InterventionFn> {
    Box::new(move |context: &Context, person_id| -> f64 {
        let property_val = context.get_person_property(person_id, T);

        for item in modifiers {
            if property_val == item.0 {
                return item.1;
            }
        }
        // Return a default 1.0 (no relative change if unregistered)
        return 1.0;
    })
}

impl ContextTransmissionModifierExt for Context {
    fn register_transmission_modifier<
        T: PersonProperty + 'static,
    >(
        &mut self,
        infectious_status: InfectiousStatusType,
        person_property: T,
        modifier_key: &Vec<(T::Value, f64)>,
    ) {
        let mut transmission_modifier_map = HashMap::new();
        transmission_modifier_map.insert(
            TypeId::of::<T>(),
            Box::new(move |context: &mut Context, person_id: PersonId| -> f64 {
                let property_val = context.get_person_property(person_id, person_property);

                for item in modifier_key {
                    if item.0 == property_val {
                        return item.1;
                    }
                }
                return 1.0;
            }),
        );

        let transmission_modifier_container =
            self.get_data_container_mut(TransmissionModifierPlugin);

        // mismatched types: instance_map is of type {closure} but expected {dyn Fn(...)}
        transmission_modifier_container
            .transmission_modifier_map
            .insert(infectious_status, transmission_modifier_map);
    }
    fn register_transmission_aggregator(
        &mut self,
        infectious_status: InfectiousStatusType,
        agg_function: Box<TransmissionAggregatorFn>,
    ) {
        let transmission_modifier_container =
            self.get_data_container_mut(TransmissionModifierPlugin);

        transmission_modifier_container
            .modifier_aggregator
            .insert(&infectious_status, agg_function);
    }

    fn compute_relative_transmission(&mut self, person_id: PersonId) -> f64 {
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        let mut registered_modifiers = Vec::new();
        let transmission_modifier_plugin =
            self.get_data_container(TransmissionModifierPlugin).unwrap();
        let transmission_modifier_map = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&infectious_status)
            .unwrap();

        for (t, f) in transmission_modifier_map {
            registered_modifiers.push((*t, f(self, person_id)));
        }

        transmission_modifier_plugin.run_aggregator(infectious_status, &registered_modifiers)
    }

    fn query_infection_modifers(&mut self, transmitter_id: PersonId, contact_id: PersonId) -> f64 {
        self.compute_relative_transmission(transmitter_id)
            * self.compute_relative_transmission(contact_id)
    }
}
