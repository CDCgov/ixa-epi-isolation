use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};
use ixa::{define_data_plugin, people::ContextPeopleExt, Context, PersonId, PersonProperty};
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

pub fn init(context: &mut Context) {
    let _ = context.get_data_container_mut(InterventionPlugin);
}

pub trait ContextInterventionExt {
    fn query_relative_transmission<T: PersonProperty + 'static>(
        &self,
        person_id: PersonId,
        intervention_type: T,
    ) -> f64;
    fn register_intervention<T: std::fmt::Debug>(
        &mut self,
        infectious_status: InfectiousStatusType,
        intervention_status: T,
        relative_transmission: f64,
    );
}

impl ContextInterventionExt for Context {
    fn query_relative_transmission<T: PersonProperty + 'static>(
        &self,
        person_id: PersonId,
        intervention_type: T,
    ) -> f64 {
        let intervention_status = self.get_person_property(person_id, intervention_type);
        let infectious_status = self.get_person_property(person_id, InfectiousStatus);

        //Relative transmission rate for facemask status, default is 1.0
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
    ) {
        let mut transmission_map = HashMap::new();

        transmission_map.insert(format!("{intervention_status:?}"), relative_transmission);

        self.get_data_container_mut(InterventionPlugin)
            .intervention_map
            .insert(infectious_status, transmission_map);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::facemask_manager::{FacemaskStatus, FacemaskStatusType};
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