use ixa::people::PersonId;
use ixa::{define_data_plugin, define_rng, Context, ContextPeopleExt, ContextRandomExt, IxaError};
use std::any::{Any, TypeId};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy)]
pub struct SettingProperties {
    alpha: f64,
}

pub trait SettingType {
    fn calculate_multiplier(
        &self,
        members: &Vec<PersonId>,
        setting_properties: &SettingProperties,
    ) -> f64;
}

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SettingId<T: SettingType + 'static> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    pub setting_type: PhantomData<*const T>,
}

impl<T: SettingType + 'static> SettingId<T> {
    pub fn new(id: usize) -> SettingId<T> {
        SettingId {
            id,
            setting_type: PhantomData,
        }
    }
}

struct ItineraryEntry {
    setting_type: TypeId,
    setting_id: usize,
    ratio: f64,
}

impl ItineraryEntry {
    fn new<T: SettingType>(setting_id: SettingId<T>, ratio: f64) -> ItineraryEntry {
        ItineraryEntry {
            setting_type: TypeId::of::<T>(),
            setting_id: setting_id.id,
            ratio,
        }
    }
}

pub struct SettingDataContainer {
    setting_types: HashMap<TypeId, Box<dyn SettingType>>,
    // For each setting type (e.g., Home) store the properties (e.g., alpha)
    setting_properties: HashMap<TypeId, SettingProperties>,
    // For each setting type, have a map of each setting id and a list of members
    members: HashMap<TypeId, HashMap<usize, Vec<PersonId>>>,
    itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
}

impl SettingDataContainer {
    fn new() -> Self {
        SettingDataContainer {
            setting_types: HashMap::new(),
            setting_properties: HashMap::new(),
            members: HashMap::new(),
            itineraries: HashMap::new(),
        }
    }
    fn get_setting_members(
        &self,
        setting_type: &TypeId,
        setting_id: &usize,        
    ) -> Option<&Vec<PersonId>> {
        self
            .members
            .get(setting_type)?
            .get(&setting_id)
    }
    fn with_itinerary<F>(&self, person_id: PersonId, mut callback: F)
    where
        F: FnMut(&dyn SettingType, &SettingProperties, &Vec<PersonId>, f64),
    {
        if let Some(itinerary) = self.itineraries.get(&person_id) {
            for entry in itinerary {
                let setting_type = self.setting_types.get(&entry.setting_type).unwrap();
                let setting_props = self.setting_properties.get(&entry.setting_type).unwrap();
                let members = self.get_setting_members(
                    &entry.setting_type,
                    &entry.setting_id).unwrap();
                callback(setting_type.as_ref(), setting_props, members, entry.ratio);
            }
        }
    }
}

// Define a home setting
#[derive(Default, Hash, Eq, PartialEq)]
pub struct Home {}

impl SettingType for Home {
    // Read members and setting_properties as arguments
    fn calculate_multiplier(
        &self,
        members: &Vec<PersonId>,
        setting_properties: &SettingProperties,
    ) -> f64 {
        let n_members = members.len();
        return ((n_members - 1) as f64).powf(setting_properties.alpha);
    }
}

#[derive(Default, Hash, Eq, PartialEq)]
pub struct CensusTract {}
impl SettingType for CensusTract {
    fn calculate_multiplier(
        &self,
        members: &Vec<PersonId>,
        setting_properties: &SettingProperties,
    ) -> f64 {
        let n_members = members.len();
        return ((n_members - 1) as f64).powf(setting_properties.alpha);
    }
}

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::new()
);

pub trait ContextSettingExt {
    fn get_setting_properties<T: SettingType + 'static>(&self) -> SettingProperties;
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting: T,
        setting_props: SettingProperties,
    );
    fn add_itinerary(&mut self, person_id: PersonId, itinerary: Vec<ItineraryEntry>) -> Result<(), IxaError>;
    fn get_setting_members<T: SettingType + 'static>(
        &self,
        setting_id: SettingId<T>,
    ) -> Option<&Vec<PersonId>>;
    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64;
    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>>;
    fn get_contact<T: SettingType + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
    ) -> Option<PersonId>;
}

impl ContextSettingExt for Context {
    fn get_setting_properties<T: SettingType + 'static>(&self) -> SettingProperties {
        let data_container = self
            .get_data_container(SettingDataPlugin)
            .unwrap()
            .setting_properties
            .get(&TypeId::of::<T>())
            .unwrap();
        return *data_container;
    }
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting_type: T,
        setting_props: SettingProperties,
    ) {
        let container = self.get_data_container_mut(SettingDataPlugin);

        // Add the setting
        container
            .setting_types
            .insert(TypeId::of::<T>(), Box::new(setting_type));

        // Add properties
        container
            .setting_properties
            .insert(TypeId::of::<T>(), setting_props);
    }
    fn add_itinerary(&mut self, person_id: PersonId, itinerary: Vec<ItineraryEntry>) -> Result<(), IxaError>{
        let container = self.get_data_container_mut(SettingDataPlugin);
        let mut setting_counts: HashMap<TypeId, HashSet<usize>> = HashMap::new();
        for itinerary_entry in itinerary.iter() {
            let setting_id = itinerary_entry.setting_id;
            let setting_type = itinerary_entry.setting_type;
            if let Some(setting_count_set) = setting_counts.get(&setting_type) {
                if setting_count_set.contains(&setting_id) {
                    return Err(IxaError::from(format!("Duplicated setting")));
                }
            }
            setting_counts
                .entry(setting_type)
                .or_insert_with(|| HashSet::new())
                .insert(setting_id);
            // TODO: If we are changing a person's itinerary, the person_id should be removed from vector
            // This isn't the same as the concept of being present or not.
            container
                .members
                .entry(itinerary_entry.setting_type)
                .or_insert_with(|| HashMap::new())
                .entry(setting_id)
                .or_insert_with(|| Vec::new()).push(person_id);
        }
        container.itineraries.insert(person_id, itinerary);
        Ok(())
    }

    fn get_setting_members<T: SettingType + 'static>(
        &self,
        setting_id: SettingId<T>,
    ) -> Option<&Vec<PersonId>> {
        self.get_data_container(SettingDataPlugin)?
            .get_setting_members(&TypeId::of::<T>(), &setting_id.id)
    }

    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut collector = 0.0;
        container.with_itinerary(person_id, |setting_type, setting_props, members, ratio| {
            let multiplier = setting_type.calculate_multiplier(members, setting_props);
            collector += ratio * multiplier;
        });
        collector
    }

    // Perhaps setting ids should include type and id so that one can have a vector of setting ids
    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        self.get_data_container(SettingDataPlugin)
            .expect("Person should be added to settings")
            .itineraries
            .get(&person_id)
    }

    fn get_contact<T: SettingType + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
    ) -> Option<PersonId> {
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
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.001 });
        let home_props = context.get_setting_properties::<Home>();
        let tract_props = context.get_setting_properties::<CensusTract>();

        assert_eq!(0.1, home_props.alpha);
        assert_eq!(0.001, tract_props.alpha);
    }

    #[test]
    fn test_duplicated_itinterary() {
        let mut context = Context::new();
        context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::<Home>::new(2), 0.5),
            ItineraryEntry::new(SettingId::<Home>::new(2), 0.5),
        ];
        
        match context.add_itinerary(person, itinerary) {
            Err(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Duplicated setting");
            }
            _ => panic!("Unexpected error in itinerary"),
        }
    }


    #[test]
    fn test_add_itinterary() {
        let mut context = Context::new();
        context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::<Home>::new(1), 0.5),
            ItineraryEntry::new(SettingId::<Home>::new(2), 0.5),
        ];
        let _ = context.add_itinerary(person, itinerary);
        let members = context.get_setting_members::<Home>(SettingId::<Home>::new(2)).unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_person(()).unwrap();
        let itinerary2 = vec![
            ItineraryEntry::new(SettingId::<Home>::new(2), 1.0),
        ];
        let _ = context.add_itinerary(person2, itinerary2);

        let members2 = context.get_setting_members::<Home>(SettingId::<Home>::new(2)).unwrap();
        assert_eq!(members2.len(), 2);
    }

    
    #[test]
    fn test_setting_registration() {
        let mut context = Context::new();
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::<Home>::new(s), 0.5),
                    ItineraryEntry::new(SettingId::<CensusTract>::new(s), 0.5),
                ];
                let _ = context.add_itinerary(person, itinerary);
            }
            let members = context.get_setting_members::<Home>(SettingId::<Home>::new(s)).unwrap();
            let tract_members = context.get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(s)).unwrap();
            // Get the number of people for these settings and should be 5
            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_multiplier() {
        // TODO: if setting not registered, shouldn't be able to register people to setting
        let mut context = Context::new();
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::<Home>::new(s), 0.5),
                ];
               let _ = context.add_itinerary(person, itinerary);
            }
        }

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::<Home>::new(home_id), 0.5),
        ];
        let _ = context.add_itinerary(person, itinerary); 
        let members = context.get_setting_members::<Home>(SettingId::<Home>::new(home_id)).unwrap();

        let setting_type = Home {};

        let inf_multiplier = setting_type.calculate_multiplier(members, &SettingProperties { alpha: 0.1 });
        
        // This is assuming we know what the function for Home is (N - 1) ^ alpha
        assert_eq!(inf_multiplier, ((6.0 - 1.0) as f64).powf(0.1));
    }


    #[test]
    fn test_total_infectiousness_multiplier() {
        // Go through all the settings and compute infectiousness multiplier
        let mut context = Context::new();
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });
        
        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::<Home>::new(s), 0.5),
                    ItineraryEntry::new(SettingId::<CensusTract>::new(s), 0.5),
                ];
                let _  = context.add_itinerary(person, itinerary);
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![
            ItineraryEntry::new(SettingId::<Home>::new(0), 1.0),
        ];
        let person = context.add_person(()).unwrap();
        let _ = context.add_itinerary(person, itinerary);

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context
            .calculate_total_infectiousness_multiplier_for_person(person);
        assert_eq!(inf_multiplier, ((6.0 - 1.0) as f64).powf(0.1));
        
        // If person's itinerary is changed for two settings,
        // CensusTract 0 should have 6 members, Home 0 should have 7 members
        // the total infectiousness should be the sum of infs * proportion
        let person = context.add_person(()).unwrap();
        let itinerary_complete = vec![
            ItineraryEntry::new(SettingId::<Home>::new(0), 0.5),
            ItineraryEntry::new(SettingId::<CensusTract>::new(0), 0.5),
        ];
        let _ = context.add_itinerary(person, itinerary_complete);
        let members_home = context.get_setting_members::<Home>(SettingId::<Home>::new(0)).unwrap();
        let members_tract = context.get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(0)).unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings = context
            .calculate_total_infectiousness_multiplier_for_person(person);

        assert_eq!(
            inf_multiplier_two_settings,
            (((7.0 - 1.0) as f64).powf(0.1)) * 0.5 + (((6.0 - 1.0) as f64).powf(0.01))*0.5
        );
    }

    #[test]
    fn test_get_contacts() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        // TODO: What happens if the person isn't registered in the setting?
        let mut context = Context::new();
        context.init_random(42);
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::<Home>::new(0), 0.5),
            ItineraryEntry::new(SettingId::<Home>::new(0), 0.5),
        ];
        let itinerary_b = vec![
            ItineraryEntry::new(SettingId::<Home>::new(0), 1.0),
        ];
        let _ = context.add_itinerary(person_a, itinerary_a);
        let _ = context.add_itinerary(person_b, itinerary_b);
        
        assert_eq!(person_b, context.get_contact::<Home>(person_a, SettingId::<Home>::new(0)).unwrap());
        assert!(context.get_contact::<CensusTract>(person_a, SettingId::<CensusTract>::new(0)).is_none());
    }
    /*TODO:
    Test failure of getting properties if not initialized
    Test failure if a setting is registered more than once?
    Test that proportions either add to 1 or that they are weighted based on proportion
    */
}
