use ixa::{
    define_data_plugin, Context, ContextPeopleExt, IxaError
};
use ixa::people::PersonId;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

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
}
 

// Define a home setting
#[derive(Hash, Eq, PartialEq)]
pub struct Home {}
impl SettingType for Home {
    fn calculate_multiplier(&self) -> f64 {
        return 10.0;
    }
}
         
#[cfg(test)]
mod test {
    use super::*;
    use crate::settings::ContextSettingExt;
    #[test]
    fn test_setting_creation() {
        let mut context = Context::new();
        context.register_setting_type::<Home>(SettingProperties{alpha: 2.0});
        let home_props = context.get_setting_properties::<Home>();

        println!("test_setting_creation:: Creating a house with alpha {}", home_props.alpha);
        assert_eq!(2.0, home_props.alpha);
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
}
