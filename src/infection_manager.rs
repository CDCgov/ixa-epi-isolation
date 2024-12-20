use crate::transmission_manager::InfectiousStatusType;
use ixa::{define_data_plugin, Context, PersonProperty};
use std::collections::HashMap;
trait Intervention<T> {
    fn map_intervention(self, property: String) -> HashMap<String, f64>;
}

trait ContextInterventionExt {
    fn register_intervention<T: PersonProperty + std::fmt::Debug>(
        &mut self,
        t: T,
        infectious_status: InfectiousStatusType,
        i: impl Intervention<T::Value>,
    );
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

// How to autmoatically iterate through the Person Property types when calculating total effects?
// This method could be keyed upon registration the intervention,
// e.g.: returning -> String = format!("{t:?}"), or appedning T as an element of some item
// This should be handled within a macro define_intervention! to generate iterative registrations

impl<T: std::fmt::Debug> Intervention<T> for Vec<(T, f64)> {
    fn map_intervention(self, property: String) -> HashMap<String, f64> {
        let mut map = HashMap::new();

        for (k, v) in self {
            map.insert(format!("{property}::{k:?}"), v);
        }
        map
    }
}

impl ContextInterventionExt for Context {
    fn register_intervention<T: PersonProperty + std::fmt::Debug>(
        &mut self,
        t: T,
        infectious_status: InfectiousStatusType,
        i: impl Intervention<T::Value>,
    ) {
        let value_map = i.map_intervention(format!("{t:?}"));

        self.get_data_container_mut(InterventionPlugin)
            .intervention_map
            .insert(infectious_status, value_map);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transmission_manager::InfectiousStatusType;
    use ixa::{define_person_property, Context};
    use std::collections::HashMap;

    #[derive(Debug, Hash, Clone, Copy, Eq, PartialEq)]
    pub enum FacemaskStatusType {
        None,
        Wearing,
    }

    #[derive(Debug, Hash, Clone, Copy, Eq, PartialEq)]
    pub enum IsolationStatusType {
        None,
        Isolating,
    }

    define_person_property!(FacemaskStatus, FacemaskStatusType);
    define_person_property!(IsolationStatus, IsolationStatusType);

    #[test]
    fn test_map_intervention() {
        let vec = vec![
            (FacemaskStatusType::None, 1.0),
            (FacemaskStatusType::Wearing, 0.5),
        ];
        let map = vec.map_intervention("FacemaskStatus".to_string());
        let mut expected = HashMap::new();
        expected.insert("FacemaskStatus::None".to_string(), 1.0);
        expected.insert("FacemaskStatus::Wearing".to_string(), 0.5);
        assert_eq!(map, expected);
    }

    #[test]
    fn test_register_intervention() {
        let mut context = Context::new();
        let vec = vec![
            (FacemaskStatusType::None, 1.0),
            (FacemaskStatusType::Wearing, 0.5),
        ];
        context.register_intervention(
            FacemaskStatus,
            InfectiousStatusType::Infectious,
            vec.clone(),
        );
        let map = &context
            .get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map;
        let mut expected = HashMap::new();
        expected.insert(
            InfectiousStatusType::Infectious,
            vec.map_intervention("FacemaskStatus".to_string()),
        );
        assert_eq!(*map, expected);
    }

    #[test]
    fn test_multiple_register_intervention() {
        let mut context = Context::new();
        let vec = vec![
            (FacemaskStatusType::None, 1.0),
            (FacemaskStatusType::Wearing, 0.5),
        ];
        let vec2 = vec![
            (IsolationStatusType::None, 0.25),
            (IsolationStatusType::Isolating, 0.75),
        ];
        context.register_intervention(
            FacemaskStatus,
            InfectiousStatusType::Infectious,
            vec.clone(),
        );
        context.register_intervention(
            IsolationStatus,
            InfectiousStatusType::Infectious,
            vec2.clone(),
        );
        let map = &context
            .get_data_container(InterventionPlugin)
            .unwrap()
            .intervention_map;
        let mut expected = HashMap::new();
        expected.insert(
            InfectiousStatusType::Infectious,
            vec.map_intervention("FacemaskStatus".to_string()),
        );
        expected.insert(
            InfectiousStatusType::Infectious,
            vec2.map_intervention("IsolationStatus".to_string()),
        );
        assert_eq!(*map, expected);
    }
}
