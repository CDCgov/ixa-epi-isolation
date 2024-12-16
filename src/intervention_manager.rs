use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{
    define_data_plugin, people::ContextPeopleExt, Context, IxaError, PersonId, PersonProperty,
};
use std::collections::HashMap;

struct InterventionContainer {
    intervention_map: HashMap<InfectiousStatusType, HashMap<String, f64>>,
}

define_data_plugin!(
    InterventionPlugin,
    InterventionContainer,
    InterventionContainer {
        intervention_map: HashMap::new(),
    }
);

/// Initializes the intervention plugin with an empty `InterventionContainer`.
pub fn init(context: &mut Context) {
    let _ = context.get_data_container_mut(InterventionPlugin);
}

pub trait ContextInterventionExt {
    /// Queries the relative transmission rate in the `InterventionPlugin` for a person under a specified intervention.
    fn query_relative_transmission<T: PersonProperty + 'static>(
        &self,
        person_id: PersonId,
        intervention_type: T,
    ) -> f64;
    /// Registers an intervention in the `InterventionPlugin` with a relative transmission effect for a particular infectious status.
    /// Default relative transmission is 1.0
    fn register_intervention<T: std::fmt::Debug>(
        &mut self,
        infectious_status: InfectiousStatusType,
        intervention_status: T,
        relative_transmission: f64,
    ) -> Result<(), IxaError>;
}

impl ContextInterventionExt for Context {
    fn query_relative_transmission<T: PersonProperty + 'static>(
        &self,
        person_id: PersonId,
        intervention_type: T,
    ) -> f64 {
        let intervention_status = self.get_person_property(person_id, intervention_type);
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        //Relative transmission rate for intervention status, default is 1.0
        *self
            .get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map
            .get(&infectious_status)
            .unwrap_or(&HashMap::new())
            .get(&format!("{intervention_status:?}"))
            .unwrap_or(&1.0)
    }

    fn register_intervention<T: std::fmt::Debug>(
        &mut self,
        infectious_status: InfectiousStatusType,
        intervention_status: T,
        relative_transmission: f64,
    ) -> Result<(), IxaError> {
        let mut transmission_map = HashMap::new();

        transmission_map.insert(format!("{intervention_status:?}"), relative_transmission);

        if !(0.0..1.0).contains(&relative_transmission) {
            return Err(IxaError::IxaError(
                "Relative transmission must be between 0.0 and 1.0.".to_string(),
            ));
        }

        self.get_data_container_mut(InterventionPlugin)
            .intervention_map
            .insert(infectious_status, transmission_map);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ixa::{define_person_property, people::ContextPeopleExt, Context};
    use std::hash::Hash;

    #[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
    pub enum FooStatusType {
        Bar,
    }

    define_person_property!(FooStatus, FooStatusType);

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_query_relative_transmission() {
        let mut context = Context::new();
        let contact_id = context.add_person((FooStatus, FooStatusType::Bar)).unwrap();

        context
            .register_intervention(InfectiousStatusType::Susceptible, FooStatusType::Bar, 0.5)
            .unwrap();

        let relative_transmission = context.query_relative_transmission(contact_id, FooStatus);

        assert_eq!(relative_transmission, 0.5);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_query_relative_transmission_default() {
        let mut context = Context::new();
        let contact_id = context.add_person((FooStatus, FooStatusType::Bar)).unwrap();
        init(&mut context);

        let relative_transmission = context.query_relative_transmission(contact_id, FooStatus);
        assert_eq!(relative_transmission, 1.0);
    }
}
