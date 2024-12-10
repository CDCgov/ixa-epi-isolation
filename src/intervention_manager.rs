//relative infectiousness trait extension

use ixa::{
    define_data_plugin,  
    define_person_property, 
    define_person_property_with_default,
     Context, 
     people::ContextPeopleExt, 
     PersonId, 
};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum FacemaskStatusType {
    None,
    Wearing,
}
struct InterventionContainer {
    intervention_map: HashMap<FacemaskStatusType, f64>,
}

define_data_plugin!(
    InterventionPlugin,
    InterventionContainer,
    InterventionContainer {
        intervention_map: HashMap::<FacemaskStatusType, f64>::new(),
    }
);

define_person_property_with_default!(FacemaskStatus, FacemaskStatusType, FacemaskStatusType::None);

pub trait ContextInterventionExt {
    fn query_relative_infectiousness(&self, transmitter_id: PersonId) -> f64;
    fn query_relative_susceptiblity(&self, contact_id: PersonId) -> f64;
    fn register_intervention(&mut self, facemask_status: FacemaskStatusType, relative_infectiousness: f64);
}

impl ContextInterventionExt for Context {
    fn query_relative_infectiousness(&self, transmitter_id: PersonId) -> f64 {
        let facemask_status = self.get_person_property(transmitter_id, FacemaskStatus);

        *self.get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map
            .get(&facemask_status)
            .unwrap()

    }

    fn query_relative_susceptiblity(&self, contact_id: PersonId) -> f64 {
        let facemask_status = self.get_person_property(contact_id, FacemaskStatus);
        match facemask_status {
            FacemaskStatusType::None => 1.0,
            FacemaskStatusType::Wearing => 0.5,
        }
    }

    fn register_intervention(&mut self, facemask_status: FacemaskStatusType, relative_infectiousness: f64) {
        self.get_data_container_mut(InterventionPlugin)
            .intervention_map
            .insert(facemask_status, relative_infectiousness);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ixa::{people::ContextPeopleExt, Context};

    #[test]
    fn test_query_relative_infectiousness() {
        let mut context = Context::new();
        let transmitter_id = context.add_person(()).unwrap();
        context.register_intervention(FacemaskStatusType::Wearing, 0.5);

        context.set_person_property(transmitter_id, FacemaskStatus, FacemaskStatusType::Wearing);

        let relative_infectiousness = context.query_relative_infectiousness(transmitter_id);
        assert_eq!(relative_infectiousness, 0.5);
    }
}
