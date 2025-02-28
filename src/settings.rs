use ixa::{
    define_data_plugin, Context, ContextPeopleExt, IxaError
};
use ixa::people::PersonId;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::marker::PhantomData;

#[allow(dead_code)]

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy)]
pub struct SettingProperties {
    alpha: f64,
}

pub trait SettingType: Any + Hash + Eq + PartialEq {
    fn calculate_multiplier(&self) -> f64;
}

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SettingId<T: SettingType> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    pub setting_type: PhantomData<*const T>,
}

pub struct SettingDataContainer {
    setting_properties: HashMap<TypeId, SettingProperties>,
    members: HashMap<TypeId, HashMap<usize, Vec<PersonId>>>,
}

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer {
        setting_properties: HashMap::default(),
        members: HashMap::default(),
    }
);

pub trait ContextSettingExt {
    fn get_setting_properties<T: SettingType>(&mut self) -> SettingProperties;
    fn register_setting_type<T: SettingType>(&mut self, setting_props: SettingProperties);
    fn add_setting<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError>;
    fn get_setting_members<T: SettingType>(&mut self, setting_id: usize) -> Option<Vec<PersonId>>;    
}

impl ContextSettingExt for Context {    
    fn get_setting_properties<T: SettingType>(&mut self) -> SettingProperties {
        let data_container = self.get_data_container(SettingDataPlugin)
            .unwrap()
            .setting_properties
            .get(&TypeId::of::<T>())
            .unwrap();
        return *data_container;
    }

    fn register_setting_type<T: SettingType>(&mut self, setting_props: SettingProperties){
        self.get_data_container_mut(SettingDataPlugin)
            .setting_properties.insert(TypeId::of::<T>(), setting_props);
    }
    
    fn add_setting<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError> {
    // First create a map if empty
    // add person id to map of type and ids
        match self.get_data_container_mut(SettingDataPlugin).members.entry(TypeId::of::<T>()) {
            Entry::Vacant(entry) => {
                let mut setting_map = HashMap::<usize, Vec<PersonId>>::new();
                setting_map.insert(setting_id, vec!(person_id));
                entry.insert(setting_map);
                Ok(())
            }
            Entry::Occupied(mut entry) => {
                // If occupied, it means already there's a setting type (e.g., home) not necessarily the setting id
                match entry.get_mut().entry(setting_id) {
                    Entry::Vacant(setting_map) => {                        
                        setting_map
                            .insert(vec!(person_id));
                    }
                    Entry::Occupied(mut setting_map) => {
                        setting_map.get_mut()
                            .push(person_id);
                    }
                }
                Ok(())
            }
        }
    }
    fn get_setting_members<T: SettingType>(&mut self, setting_id: usize) -> Option<Vec<PersonId>> {
        let setting_container = self.get_data_container(SettingDataPlugin)
            .unwrap()
            .members
            .get(&TypeId::of::<T>());

        match setting_container {
            Some(setting_map) => {
                return setting_map.get(&setting_id).cloned();
            },
            None => None
        }        
    }
}
 

// Define a home setting
#[derive(Hash, Eq, PartialEq)]
pub struct Home {}
impl SettingType for Home {
    // Read members and setting_properties as arguments
    fn calculate_multiplier(&self) -> f64 {
        return 10.0;
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use crate::settings::ContextSettingExt;
    // Define a home setting
    #[derive(Hash, Eq, PartialEq)]
    pub struct CensusTract {}
    impl SettingType for CensusTract {
        fn calculate_multiplier(&self) -> f64 {
            return 10.0;
        }
    }

    #[test]
    fn test_setting_type_creation() {
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        context.register_setting_type::<CensusTract>(SettingProperties{alpha: 0.001});
        let home_props = context.get_setting_properties::<Home>();
        let tract_props = context.get_setting_properties::<CensusTract>();

        println!("test_setting_type_creation:: Creating  house type with alpha {}", home_props.alpha);
        println!("test_setting_type_creation:: Creating censustract type with alpha {}", tract_props.alpha);
        assert_eq!(0.1, home_props.alpha);
        assert_eq!(0.001, tract_props.alpha);
    }

    #[test]
    fn test_setting_registration() {
        // TODO: if setting not registered, shouldn't be able to register setting
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        let home_props = context.get_setting_properties::<Home>();
        // Create 5 people
        for _ in 0..5 {
            let person = context.add_person(()).unwrap();
            // Add them all to the same setting (Home)
            let _ = context.add_setting::<Home>(1, person);
        }

        println!("test_setting_registration:: Registring people to house with alpha {}", home_props.alpha);
        let members = context.get_setting_members::<Home>(1).unwrap();
        assert_eq!(members.len(), 5);
        println!("Members of house 1 are {:#?}", members);
        // Get the number of people for this house and should be 5
        /*
        // Registration
        context.register_setting_type::<Home>(SettingProperties);
        let setting_id: SettingId<Home> = context.add_setting::<Home>(id: String);

        context.add_person_to_setting(person, setting_id);
        // Internally this checks if a matching home exists and creates it first if not
        // it should convert a representation of a setting id from a csv file probably by
        // hashing it at some point
        context.add_person_to_predefined_setting::<Home>(person1, 123);
        context.add_person_to_predefined_setting::<Home>(person2, 123);
         */
    }
    /*
    Test failure of getting properties if not initialized
    Test failure if a setting is registered more than once? 
    */
}
