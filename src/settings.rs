use ixa::{
    define_data_plugin, Context, ContextPeopleExt, IxaError,
    define_rng, ContextRandomExt
};
use ixa::people::PersonId;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::marker::PhantomData;
use tinyset::SetUsize;

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy)]
pub struct SettingProperties {
    alpha: f64,
}


pub trait SettingType {
    fn calculate_multiplier(&self, context: &Context, setting_id: usize, setting_properties: &SettingProperties) -> f64;
}

struct ItineraryEntry {
    setting_type: Box<dyn SettingType>,
    setting_id: usize,
    ratio: f64
}

pub struct SettingDataContainer {
    setting_properties: HashMap<TypeId, SettingProperties>,
    members: HashMap<TypeId, HashMap<usize, Vec<PersonId>>>,
    members_to_settings: HashMap<PersonId, HashMap<TypeId,SetUsize>>,
    itineraries: HashMap<PersonId, Vec<ItineraryEntry>>
}

// Define a home setting
#[derive(Default, Hash, Eq, PartialEq)]
pub struct Home {}

impl SettingType for Home {
    // Read members and setting_properties as arguments
    fn calculate_multiplier(context: &Context, setting_id: usize, setting_properties: &SettingProperties) -> f64 {
        let n_members = context.get_setting_members::<Self>(setting_id).unwrap().len();
        return ((n_members - 1) as f64).powf(setting_properties.alpha);
    }
}

#[derive(Default, Hash, Eq, PartialEq)]
pub struct CensusTract {}
impl SettingType for CensusTract {
    fn calculate_multiplier(context: &Context, setting_id: usize, setting_properties: &SettingProperties) -> f64 {
        let n_members = context.get_setting_members::<Self>(setting_id).unwrap().len();
        return ((n_members - 1) as f64).powf(setting_properties.alpha);
    }
}


define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer {
        setting_properties: HashMap::default(),
        members: HashMap::default(),
        members_to_settings: HashMap::default(),
        itineraries: HashMap::default()
    }
);

pub trait ContextSettingExt {
    fn get_setting_properties<T: SettingType>(&self) -> SettingProperties;
    fn register_setting_type<T: SettingType>(&mut self, setting_props: SettingProperties);
    fn add_setting<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError>;
    fn register_setting_for_person<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError>;
    // TODO: To get setting members, how can we query instead?(e.g., filter Alive.)
    fn get_setting_members<T: SettingType>(&self, setting_id: usize) -> Option<Vec<PersonId>>;
    fn calculate_total_infectiousness_multiplier_for_setting<T: SettingType>(&self, setting_id: usize) -> f64;
    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> Option<f64>;
    fn get_itinerary(&self, person_id: PersonId) -> Option<Vec<ItineraryEntry>>;
    fn get_contact<T: SettingType>(&self, person_id: PersonId, setting_id: usize) -> Option<PersonId>;
}

impl ContextSettingExt for Context {    
    fn get_setting_properties<T: SettingType>(&self) -> SettingProperties {
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

    fn register_setting_for_person<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError> {
        match self.get_data_container_mut(SettingDataPlugin).members_to_settings.entry(person_id) {
            Entry::Vacant(entry) => {
                let mut setting_map = HashMap::new();
                let mut new_setting_set = SetUsize::new();
                new_setting_set.insert(setting_id);
                setting_map.insert(TypeId::of::<T>(),new_setting_set);
                entry.insert(setting_map);
                Ok(())
            }
            Entry::Occupied(mut entry) => {
                // If occupied, it means already there's a setting type (e.g., home) not necessarily the setting id
                match entry.get_mut().entry(TypeId::of::<T>()) {
                    Entry::Vacant(setting_map) => {
                        let mut new_setting_set = SetUsize::new();
                        new_setting_set.insert(setting_id);
                        setting_map.insert(new_setting_set);
                    }
                    Entry::Occupied(mut setting_map) => {
                        setting_map.get_mut()
                            .insert(setting_id);
                    }
                }
                Ok(())
            }
        }
    }
    //TODO: person_ids should probably be a set and not a vec to avoid duplicates
    fn add_setting<T: SettingType>(&mut self, setting_id: usize, person_id: PersonId) -> Result<(), IxaError> {
    // First create a map if empty
    // add person id to map of type and ids
        match self.get_data_container_mut(SettingDataPlugin).members.entry(TypeId::of::<T>()) {
            Entry::Vacant(entry) => {
                let mut setting_map = HashMap::<usize, Vec<PersonId>>::new();
                setting_map.insert(setting_id, vec!(person_id));
                entry.insert(setting_map);
                self.register_setting_for_person::<T>(setting_id, person_id)?;
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
                self.register_setting_for_person::<T>(setting_id, person_id)?;
                Ok(())
            }
        }
    }
    fn get_setting_members<T: SettingType + 'static>(&self, setting_id: usize) -> Option<Vec<PersonId>> {
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

    fn calculate_total_infectiousness_multiplier_for_setting<T: SettingType>(&self, setting, setting_id: usize) -> f64 {
        let setting_properties = self.get_setting_properties::<T>();
        let setting = 
        return setting.calculate_multiplier(self, setting_id,  &setting_properties);
    }

    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> Option<f64> {
        // TODO: This will probably change for a person property implementation or something else
        //       Needs to incorporate proportion of time to compute infectiousness by setting id
        if let Some(itinerary) = self.get_itinerary(person_id) {
            let mut total_infectiousness = 0.0;
            // Go through each registered setting and calculate total infectiousness
            for itinerary_entry in itinerary.into_iter() {
                for id in setting_ids.iter() {
                    // I know this is wrong, but some hacky way of doing this for now
                    total_infectiousness += match setting_type {
                        t if t == TypeId::of::<Home>() =>  self.calculate_total_infectiousness_multiplier_for_setting::<Home>( id),
                        t if t == TypeId::of::<CensusTract>() => self.calculate_total_infectiousness_multiplier_for_setting::<CensusTract>( id),
                        _ => 0.0,
                    }
                }
            }
            Some(total_infectiousness)
        } else {
            None
        }
    }
    
    // Perhaps setting ids should include type and id so that one can have a vector of setting ids
    fn get_itinerary(&self, person_id: PersonId) -> Option<Vec<ItineraryEntry>> {
        self.get_data_container(SettingDataPlugin)
            .expect("Person should be added to settings")
            .itineraries
            .get(&person_id)
    }

    fn get_contact<T: SettingType>(&self, person_id: PersonId, setting_id: usize) -> Option<PersonId> {
        if let Some(members) = self.get_setting_members::<T>(setting_id) {
            if members.len() == 1 {
                return None;
            }
            let mut contact_id = person_id;
            while contact_id == person_id {
                contact_id = members[self.sample_range(SettingsRng, 0..members.len())];
            }
            Some(contact_id)
        } else {
            None
        }
        
        
    }
}
 


#[cfg(test)]
mod test {
    use super::*;
    use crate::settings::ContextSettingExt;
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

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let _ = context.add_setting::<Home>(home_id, person);

        let inf_multiplier = context.calculate_total_infectiousness_multiplier_for_setting::<Home>(home_id);
        let members = context.get_setting_members::<Home>(home_id).unwrap();
        println!("Setting multiplier {inf_multiplier} with members  {:#?}", members);
        assert_eq!(inf_multiplier, ((6.0 - 1.0) as f64).powf(0.1));
    }

    #[test]
    fn test_person_settings() {
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        context.register_setting_type::<CensusTract>(SettingProperties{alpha: 0.01});
        // Create 5 people
        for _ in 0..5 {
            let person = context.add_person(()).unwrap();
            let _ = context.add_setting::<Home>(0, person);
            let _ = context.add_setting::<CensusTract>(0, person);
        }
        // Get all settings a person is registered
        // Every person should be registered to home 0 and census tract 0
        let person = context.add_person(()).unwrap();
        let _ = context.add_setting::<Home>(0, person);
        let _ = context.add_setting::<CensusTract>(0, person);
        let _ = context.add_setting::<CensusTract>(1, person);
        let person_settings = context.get_itinerary(person).unwrap();
        let mut home_ids = SetUsize::new();
        home_ids.insert(0);
        let mut tract_ids = SetUsize::new();
        tract_ids.insert(0);
        tract_ids.insert(1);
        
        println!("Person settings for {person} {:#?}", person_settings.get(&TypeId::of::<Home>()));
        println!("Settings for person {person}: {:#?}", person_settings);

        assert_eq!(home_ids, person_settings.get(&TypeId::of::<Home>()).unwrap().clone());
        assert_eq!(tract_ids, person_settings.get(&TypeId::of::<CensusTract>()).unwrap().clone());
    }

    #[test]
    fn test_total_infectiousness_multiplier() {
        // Go through all the settings and compute infectiousness multiplier
        // First check only one setting, then check a person in multiple settings
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        context.register_setting_type::<CensusTract>(SettingProperties{alpha: 0.01});
        // Create 5 people
        for _ in 0..5 {
            let person = context.add_person(()).unwrap();
            let _ = context.add_setting::<Home>(0, person);
            let _ = context.add_setting::<CensusTract>(0, person);
        }
        // Add three more people to census tract 1 
        for _ in 0..3 {
            let person = context.add_person(()).unwrap();
             let _ = context.add_setting::<CensusTract>(1, person);
        }
        
        // Get all settings a person is registered
        // Create a new person and register to home 0 
        let person = context.add_person(()).unwrap();
        let _ = context.add_setting::<Home>(0, person);

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context.calculate_total_infectiousness_multiplier_for_person(person).unwrap();
        println!("Total infectiousness with one setting: {inf_multiplier}");
        assert_eq!(inf_multiplier, ((6.0 - 1.0) as f64).powf(0.1));

        // If another setting with other 6 people are added, the total infectiousness should be the sum of infs
        // Censustract alpha here defined as 0.01
        let _ = context.add_setting::<CensusTract>(0, person);
        let inf_multiplier_two_settings = context.calculate_total_infectiousness_multiplier_for_person(person).unwrap();
        println!("Total infectiousness with two settings: {inf_multiplier_two_settings}");
        assert_eq!(inf_multiplier_two_settings, ((6.0 - 1.0) as f64).powf(0.1) + ((6.0 - 1.0) as f64).powf(0.01));
        
        let _ = context.add_setting::<CensusTract>(1, person);
        let inf_multiplier_three_settings = context.calculate_total_infectiousness_multiplier_for_person(person).unwrap();
        // Adding a third setting to the person but this setting has only 4 people, not 6
        println!("Total infectiousness with three settings: {inf_multiplier_three_settings}");
        assert_eq!(inf_multiplier_three_settings, ((6.0 - 1.0) as f64).powf(0.1) + ((6.0 - 1.0) as f64).powf(0.01) + ((4.0 - 1.0) as f64).powf(0.01));

        
    }

    #[test]
    fn test_get_contacts() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        // TODO: What happens if the person isn't registered in the setting? 
        let mut context = Context::new();
        context.init_random(42);
        context.register_setting_type::<Home>(SettingProperties{alpha: 0.1});
        context.register_setting_type::<CensusTract>(SettingProperties{alpha: 0.01});
        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let _ = context.add_setting::<Home>(0, person_a);
        let _ = context.add_setting::<Home>(0, person_b);
        let _ = context.add_setting::<CensusTract>(0, person_a);
        assert_eq!(person_b,context.get_contact::<Home>(person_a, 0).unwrap());
        assert!(context.get_contact::<CensusTract>(person_a, 0).is_none());
    }
    
    /*TODO:
    Test failure of getting properties if not initialized
    Test failure if a setting is registered more than once? 
    */
}
