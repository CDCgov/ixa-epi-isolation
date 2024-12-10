use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{
    define_data_plugin, define_person_property, define_person_property_with_default,
    people::ContextPeopleExt, Context, PersonId,
};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum FacemaskStatusType {
    None,
    Wearing,
}
struct InterventionContainer {
    intervention_map: HashMap<InfectiousStatusType, HashMap<FacemaskStatusType, f64>>,
}

define_data_plugin!(
    InterventionPlugin,
    InterventionContainer,
    InterventionContainer {
        intervention_map: HashMap::new(),
    }
);

define_person_property_with_default!(FacemaskStatus, FacemaskStatusType, FacemaskStatusType::None);

pub trait ContextInterventionExt {
    fn query_relative_transmission(&self, person_id: PersonId) -> f64;
    fn register_intervention(
        &mut self,
        infectious_status: InfectiousStatusType,
        facemask_status: FacemaskStatusType,
        relative_transmission: f64,
    );
}

impl ContextInterventionExt for Context {
    fn query_relative_transmission(&self, person_id: PersonId) -> f64 {
        let facemask_status = self.get_person_property(person_id, FacemaskStatus);
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        *self
            .get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map
            .get(&infectious_status)
            .unwrap()
            .get(&facemask_status)
            .unwrap()
    }

    fn register_intervention(
        &mut self,
        infectious_status: InfectiousStatusType,
        facemask_status: FacemaskStatusType,
        relative_transmission: f64,
    ) {
        let mut facemask_map = HashMap::new();

        facemask_map.insert(facemask_status, relative_transmission);

        self.get_data_container_mut(InterventionPlugin)
            .intervention_map
            .insert(infectious_status, facemask_map);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ixa::{people::ContextPeopleExt, Context};

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_query_relative_transmission() {
        let mut context = Context::new();
        let contact_id = context.add_person(()).unwrap();

        context.register_intervention(
            InfectiousStatusType::Susceptible,
            FacemaskStatusType::Wearing,
            0.5,
        );
        context.set_person_property(contact_id, FacemaskStatus, FacemaskStatusType::Wearing);
        let relative_transmission = context.query_relative_transmission(contact_id);

        assert_eq!(relative_transmission, 0.5);
    }
}
