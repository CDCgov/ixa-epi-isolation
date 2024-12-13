use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{
    define_data_plugin, define_person_property, define_person_property_with_default,
    people::ContextPeopleExt, Context, PersonId, PersonProperty,
};
use std::{collections::HashMap, hash::Hash};

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum FacemaskStatusType {
    None,
    Wearing,
}
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

define_person_property_with_default!(FacemaskStatus, FacemaskStatusType, FacemaskStatusType::None);

pub fn init(context: &mut Context) {
    let _ = context.get_data_container_mut(InterventionPlugin);
}

pub trait ContextInterventionExt {
    fn query_relative_transmission<T: PersonProperty + 'static>(&self, person_id: PersonId, intervention_type: T) -> f64;
    fn register_facemask(
        &mut self,
        infectious_status: InfectiousStatusType,
        facemask_status: FacemaskStatusType,
        relative_transmission: f64,
    );
}

impl ContextInterventionExt for Context {
    fn query_relative_transmission<T: PersonProperty + 'static>(&self, person_id: PersonId, intervention_type: T) -> f64 {
        let intervention_status = format!("{:?}", self.get_person_property(person_id, intervention_type));
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        //Relative transmission rate for facemask status, default is 1.0
        *self
            .get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map
            .get(&infectious_status)
            .unwrap_or(&HashMap::new())
            .get(&intervention_status)
            .unwrap_or(&1.0)
    }

    fn register_facemask(
        &mut self,
        infectious_status: InfectiousStatusType,
        facemask_status: FacemaskStatusType,
        relative_transmission: f64,
    ) {
        let mut facemask_map = HashMap::new();

        facemask_map.insert(format!("{:?}",facemask_status), relative_transmission);

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

        context.register_facemask(
            InfectiousStatusType::Susceptible,
            FacemaskStatusType::Wearing,
            0.5,
        );
        context.set_person_property(contact_id, FacemaskStatus, FacemaskStatusType::Wearing);
        let relative_transmission = context.query_relative_transmission(contact_id, FacemaskStatus);

        assert_eq!(relative_transmission, 0.5);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_query_relative_transmission_default() {
        let mut context = Context::new();
        let contact_id = context.add_person(()).unwrap();
        init(&mut context);

        context.set_person_property(contact_id, FacemaskStatus, FacemaskStatusType::None);
        let relative_transmission = context.query_relative_transmission(contact_id, FacemaskStatus);
        assert_eq!(relative_transmission, 1.0);
    }
}
