use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{define_data_plugin, Context, ContextPeopleExt, PersonId, PersonProperty};
use std::{any::TypeId, boxed::Box, collections::HashMap};

type InterventionFn = dyn Fn(&Context, PersonId) -> f64;
type AggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;

struct InterventionContainer {
    intervention_map: HashMap<InfectiousStatusType, HashMap<TypeId, Box<InterventionFn>>>,
    aggregator: HashMap<InfectiousStatusType, Box<AggregatorFn>>,
}

impl InterventionContainer {
    fn run_aggregator(
        &self,
        infectious_status: InfectiousStatusType,
        interventions: &Vec<(TypeId, f64)>,
    ) -> f64 {
        self.aggregator
            .get(&infectious_status)
            .unwrap_or(&Self::default_aggregator())(interventions)
    }

    fn default_aggregator() -> Box<AggregatorFn> {
        Box::new(|interventions: &Vec<(TypeId, f64)>| -> f64 {
            let mut aggregate_effects = 1.0;

            for (_, effect) in interventions {
                aggregate_effects *= effect;
            }

            aggregate_effects
        })
    }
}

define_data_plugin!(
    InterventionPlugin,
    InterventionContainer,
    InterventionContainer {
        intervention_map: HashMap::new(),
        aggregator: HashMap::new(),
    }
);

trait ContextTransmissionModifierExt {
    fn register_intervention<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(
        &mut self,
        infectious_status: InfectiousStatusType,
        person_property: T,
        instance_dict: Vec<(T::Value, f64)>,
    );
    fn register_aggregator(
        &mut self,
        agg_functions: Vec<(InfectiousStatusType, Box<AggregatorFn>)>,
    );
    fn compute_intervention(&mut self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn register_intervention<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(
        &mut self,
        infectious_status: InfectiousStatusType,
        person_property: T,
        instance_dict: Vec<(T::Value, f64)>,
    ) {
        let mut instance_map = HashMap::new();
        instance_map.insert(
            TypeId::of::<T>(),
            move |context: &mut Context, person_id| -> f64 {
                let property_val = context.get_person_property(person_id, person_property);

                for item in instance_dict {
                    if property_val == item.0 {
                        return item.1;
                    }
                }
                // Return a default 1.0 (no relative change if unregistered)
                return 1.0;
            },
        );

        let intervention_container = self.get_data_container_mut(InterventionPlugin);

        intervention_container
            .intervention_map
            .insert(infectious_status, instance_map);
    }

    fn register_aggregator(
        &mut self,
        agg_functions: Vec<(InfectiousStatusType, Box<AggregatorFn>)>,
    ) {
        let intervention_container = self.get_data_container_mut(InterventionPlugin);

        for item in agg_functions {
            intervention_container.aggregator.insert(item.0, item.1);
        }
    }

    fn compute_intervention(&mut self, person_id: PersonId) -> f64 {
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        let mut registered_interventions: Vec<(TypeId, f64)> = Vec::new();
        let intervention_plugin = self.get_data_container(InterventionPlugin).unwrap();
        let intervention_map = intervention_plugin
            .intervention_map
            .get(&infectious_status)
            .unwrap();

        for (t, f) in intervention_map {
            registered_interventions.push((*t, f(self, person_id)));
        }

        intervention_plugin.run_aggregator(infectious_status, &registered_interventions)
    }
}

pub fn query_modifers(context: &mut Context, transmitter_id: PersonId, contact_id: PersonId) -> f64 {
    context.compute_intervention(transmitter_id) * context.compute_intervention(contact_id)
}
