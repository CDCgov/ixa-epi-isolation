use ixa::{
    define_data_plugin, Context, ContextPeopleExt, IxaError,
    define_rng
};
use ixa::people::PersonId;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::marker::PhantomData;

#[allow(dead_code)]

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy)]
pub struct SettingProperties {
    alpha: f64,
}


pub trait SettingType: Any + Hash + Eq + PartialEq {
    fn new() -> Self;
    fn calculate_multiplier(&self, n_members: usize, setting_properties: &SettingProperties) -> f64;
}


// TODO: Use setting id instead of usize id. 
#[derive(Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SettingId<T: SettingType> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    pub setting_type: PhantomData<*const T>,
}

impl<T: SettingType> SettingId<T> {
    pub fn new(id: usize) -> SettingId<T> {
        SettingId {
            id,
            setting_type: PhantomData,
        }
    }
}

pub struct SettingDataContainer {
    setting_properties: HashMap<TypeId, SettingProperties>,
    members: HashMap<TypeId, HashMap<usize, Vec<PersonId>>>,
}

// Define a home setting
#[derive(Hash, Eq, PartialEq)]
pub struct Home {}

impl SettingType for Home {
    fn new() -> Self {
        Home {}
    }
    // Read members and setting_properties as arguments
    fn calculate_multiplier(&self, n_members: usize, setting_properties: &SettingProperties) -> f64 {
        return ((n_members - 1) as f64).powf(setting_properties.alpha);
    }
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
    // TODO: To get setting members, how can we query instead?(e.g., filter Alive.)
    fn get_setting_members<T: SettingType>(&mut self, setting_id: usize) -> Option<Vec<PersonId>>;
    fn calculate_infectiousness_multiplier<T: SettingType>(&mut self, person_id: PersonId, setting_id: usize) -> f64;
    // fn get_contact<T: SettingType>(&mut self, person_id: PersonId, setting_id: usize) -> Option<PersonId>;
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

    fn calculate_infectiousness_multiplier<T: SettingType>(&mut self, person_id: PersonId, setting_id: usize) -> f64 {
        let members = self.get_setting_members::<T>(setting_id).unwrap();
        let setting_properties = self.get_setting_properties::<T>();
        let setting  = T::new();
        return setting.calculate_multiplier(members.len(), &setting_properties);
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
        fn new() -> Self {
            CensusTract {}
        }
        fn calculate_multiplier(&self, n_members: usize, setting_properties: &SettingProperties) -> f64 {
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
        // TODO: if setting not registered, shouldn't be able to register people to setting
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        context.register_setting_type::<CensusTract>(SettingProperties{alpha: 0.001});
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                // Add them all to the same setting (Home)
                let _ = context.add_setting::<Home>(s, person);
                let _ = context.add_setting::<CensusTract>(s, person);
            }
            let members = context.get_setting_members::<Home>(s).unwrap();
            let tract_members = context.get_setting_members::<CensusTract>(s).unwrap();
            // Get the number of people for this house and should be 5
            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);            
            println!("Members of house {s} are {:#?} - CensusTract {:#?}", members, tract_members);
        }

    }

    #[test]
    fn test_setting_multiplier() {
        // TODO: if setting not registered, shouldn't be able to register people to setting
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        for h in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                // Add them all to the same setting (Home)
                let _ = context.add_setting::<Home>(h, person);
            }
        }

        /*
        - Get a few people
        - For each person, get their setting id for home
        - get members of home id
        - calculate infectiousness multiplier for home id (N - 1) ^ alpha
        */
        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let _ = context.add_setting::<Home>(home_id, person);

        let inf_multiplier = context.calculate_infectiousness_multiplier::<Home>(person, home_id);
        let members = context.get_setting_members::<Home>(home_id).unwrap();
        println!("Setting multiplier {inf_multiplier} with members  {:#?}", members);
        
    }
    /*TODO:
    Test failure of getting properties if not initialized
    Test failure if a setting is registered more than once? 
    */
}
