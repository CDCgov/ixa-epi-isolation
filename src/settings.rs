use crate::parameters::{ContextParametersExt, CoreSettingsTypes, ItineraryWriteFnType, Params};
use ixa::people::PersonId;
use ixa::{
    define_data_plugin, define_rng, people::Query, Context, ContextPeopleExt, ContextRandomExt,
    IxaError,
};

use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy)]
pub struct SettingProperties {
    pub alpha: f64,
}

pub trait SettingType {
    fn calculate_multiplier(
        &self,
        members: &[PersonId],
        setting_properties: SettingProperties,
    ) -> f64;
}

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SettingId<T: SettingType + 'static> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    pub setting_type: PhantomData<*const T>,
}

#[allow(dead_code)]
impl<T: SettingType + 'static> SettingId<T> {
    pub fn new(id: usize) -> SettingId<T> {
        SettingId {
            id,
            setting_type: PhantomData,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ItineraryEntry {
    setting_type: TypeId,
    setting_id: usize,
    ratio: f64,
}

type ItineraryEntryWriter = dyn Fn(&Context, TypeId, usize) -> Result<ItineraryEntry, IxaError>;

/// Creates an itinerary for use by `context.add_itinerary(PersonId, Vec<ItineraryEntry>)` based on
/// the provided settings and the set itinerary creation rules specified in the `itinerary_fn_type`
/// parameter.
pub fn create_itinerary(
    context: &Context,
    setting_id_vec: Vec<(TypeId, usize)>,
) -> Result<Vec<ItineraryEntry>, IxaError> {
    let writer = context.get_itinerary_write_rules();
    let mut itinerary = vec![];
    for (setting_type, id) in setting_id_vec {
        // Our population loader model is hard coded to put people into the settings of home,
        // school, work, and census tract. However, sometimes, we don't want all those settings
        // but rather just the ones that are specified in the input file.
        if context
            .get_data_container(SettingDataPlugin)
            .expect("Settings must be initialized prior to making itineraries")
            .setting_types
            .contains_key(&setting_type)
        {
            itinerary.push(writer(context, setting_type, id)?);
        }
    }
    Ok(itinerary)
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
        setting_id: usize,
    ) -> Option<&Vec<PersonId>> {
        self.members.get(setting_type)?.get(&setting_id)
    }
    fn with_itinerary<F>(&self, person_id: PersonId, mut callback: F)
    where
        F: FnMut(&dyn SettingType, &SettingProperties, &Vec<PersonId>, f64),
    {
        if let Some(itinerary) = self.itineraries.get(&person_id) {
            for entry in itinerary {
                let setting_type = self.setting_types.get(&entry.setting_type).unwrap();
                let setting_props = self.setting_properties.get(&entry.setting_type).unwrap();
                let members = self
                    .get_setting_members(&entry.setting_type, entry.setting_id)
                    .unwrap();
                callback(setting_type.as_ref(), setting_props, members, entry.ratio);
            }
        }
    }
}

#[macro_export]
macro_rules! define_setting_type {
    ($name:ident) => {
        #[derive(Default, Debug, Hash, Eq, PartialEq)]
        pub struct $name {}

        impl $crate::settings::SettingType for $name {
            fn calculate_multiplier(
                &self,
                members: &[ixa::PersonId],
                setting_properties: $crate::settings::SettingProperties,
            ) -> f64 {
                let n_members = members.len();
                #[allow(clippy::cast_precision_loss)]
                ((n_members - 1) as f64).powf(setting_properties.alpha)
            }
        }
    };
}
pub use define_setting_type;

define_setting_type!(Home);
define_setting_type!(CensusTract);
define_setting_type!(School);
define_setting_type!(Workplace);

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::new()
);

#[allow(dead_code)]
pub trait ContextSettingExt {
    fn get_setting_properties<T: SettingType + 'static>(
        &self,
    ) -> Result<SettingProperties, IxaError>;
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting: T,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError>;
    fn validate_setting_registration<T: SettingType + 'static>(
        &self,
        setting_type: &T,
    ) -> Result<(), IxaError>;
    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError>;
    fn validate_itinerary(&self, itinerary: &[ItineraryEntry]) -> Result<(), IxaError>;
    fn get_setting_members<T: SettingType + 'static>(
        &self,
        setting_id: SettingId<T>,
    ) -> Option<&Vec<PersonId>>;
    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64;
    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>>;
    fn get_contact<T: SettingType + 'static, Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
        q: Q,
    ) -> Option<PersonId>;
    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Option<PersonId>;
}

trait ContextSettingInternalExt {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting_type: TypeId,
        setting_id: usize,
        q: T,
    ) -> Option<PersonId>;
    fn get_setting_members_internal(
        &self,
        setting_type: TypeId,
        setting_id: usize,
    ) -> Option<&Vec<PersonId>>;
    fn get_itinerary_write_rules(&self) -> Box<ItineraryEntryWriter>;
}

impl ContextSettingInternalExt for Context {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting_type: TypeId,
        setting_id: usize,
        q: T,
    ) -> Option<PersonId> {
        self.get_setting_members_internal(setting_type, setting_id)
            .and_then(|members| {
                if members.len() == 1 {
                    return None;
                }
                let member_iter = members.iter().filter(|&x| *x != person_id);

                let mut contacts = vec![];
                if q.get_query().is_empty() {
                    // If the query is empty we push members directly to the vector
                    for contact in member_iter {
                        contacts.push(*contact);
                    }
                } else {
                    // If the query is not empty, we match setting members to the query
                    for contact in member_iter {
                        if self.match_person(*contact, q) {
                            contacts.push(*contact);
                        }
                    }
                }

                if contacts.is_empty() {
                    return None;
                }

                Some(contacts[self.sample_range(SettingsRng, 0..contacts.len())])
            })
    }
    fn get_setting_members_internal(
        &self,
        setting_type: TypeId,
        setting_id: usize,
    ) -> Option<&Vec<PersonId>> {
        self.get_data_container(SettingDataPlugin)?
            .get_setting_members(&setting_type, setting_id)
    }
    fn get_itinerary_write_rules(&self) -> Box<ItineraryEntryWriter> {
        let &Params {
            itinerary_fn_type, ..
        } = self.get_params();

        match itinerary_fn_type {
            ItineraryWriteFnType::SplitEvenly => Box::new(|_context, setting_type, setting_id| {
                Ok(ItineraryEntry {
                    setting_type,
                    setting_id,
                    ratio: 1.0,
                })
            }),
            ItineraryWriteFnType::Split {
                home,
                school,
                workplace,
                census_tract,
            } => {
                Box::new(move |_context, setting_type, setting_id| {
                    match setting_type {
                        t if t == TypeId::of::<Home>() => Ok(ItineraryEntry {
                            setting_type,
                            setting_id,
                            ratio: home,
                        }),
                        t if t == TypeId::of::<School>() => Ok(ItineraryEntry {
                            setting_type,
                            setting_id,
                            ratio: school,
                        }),
                        t if t == TypeId::of::<Workplace>() => Ok(ItineraryEntry {
                            setting_type,
                            setting_id,
                            ratio: workplace,
                        }),
                        t if t == TypeId::of::<CensusTract>() => Ok(ItineraryEntry {
                            setting_type,
                            setting_id,
                            ratio: census_tract,
                        }),
                        // For any other type id, we don't know how to make a ratio because it wasn't
                        // specified, so we raise an error.
                        _ => Err(IxaError::IxaError(
                            "The `Split` itinerary write method only supports ratios in core setting types.
                            A non core setting type was provided.".to_string(),
                        )),
                    }
                })
            }
        }
    }
}

impl ContextSettingExt for Context {
    fn get_setting_properties<T: SettingType + 'static>(
        &self,
    ) -> Result<SettingProperties, IxaError> {
        let data_container = self.get_data_container(SettingDataPlugin).unwrap();

        let registered_setting = data_container.setting_types.get(&TypeId::of::<T>());

        if registered_setting.is_none() {
            return Err(IxaError::from(
                "Attempting to get properties of unregistered setting type",
            ));
        }
        let properties = data_container
            .setting_properties
            .get(&TypeId::of::<T>())
            .unwrap();
        Ok(*properties)
    }
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting_type: T,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError> {
        self.validate_setting_registration(&setting_type)?;
        let container = self.get_data_container_mut(SettingDataPlugin);

        // Add the setting
        container
            .setting_types
            .insert(TypeId::of::<T>(), Box::new(setting_type));

        // Add properties
        container
            .setting_properties
            .insert(TypeId::of::<T>(), setting_props);
        Ok(())
    }
    fn validate_setting_registration<T: SettingType + 'static>(
        &self,
        _setting_type: &T,
    ) -> Result<(), IxaError> {
        let data_container = self.get_data_container(SettingDataPlugin);
        if let Some(data_container) = data_container {
            let registered_setting = data_container.setting_types.get(&TypeId::of::<T>());

            if registered_setting.is_some() {
                return Err(IxaError::from("Setting type is already registered"));
            }
        }
        Ok(())
    }
    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        self.validate_itinerary(&itinerary)?;
        let container = self.get_data_container_mut(SettingDataPlugin);
        let mut current_setting_ids = vec![];

        for itinerary_entry in &itinerary {
            // TODO: If we are changing a person's itinerary, the person_id should be removed from vector
            // This isn't the same as the concept of being present or not.
            let registered_setting = container.setting_types.get(&itinerary_entry.setting_type);
            if registered_setting.is_none() {
                return Err(IxaError::from(
                    "Itinerary entry setting type not registered",
                ));
            }
            container
                .members
                .entry(itinerary_entry.setting_type)
                .or_default()
                .entry(itinerary_entry.setting_id)
                .or_default()
                .push(person_id);
            current_setting_ids.push((itinerary_entry.setting_type, itinerary_entry.setting_id));
        }

        // Clean up settings that the person is no longer a member of
        if let Some(previous_itinerary) = container.itineraries.insert(person_id, itinerary) {
            // Remove the person from the previous itinerary entries not modified already
            for itinerary_entry in previous_itinerary {
                //Check if the entry of setting ID and type is not in the current itinerary
                if !current_setting_ids
                    .contains(&(itinerary_entry.setting_type, itinerary_entry.setting_id))
                {
                    // Remove the person from the previous itinerary
                    container
                        .members
                        .entry(itinerary_entry.setting_type)
                        .or_default()
                        .entry(itinerary_entry.setting_id)
                        .or_default()
                        .retain(|&x| x != person_id);
                }
            }
        }
        Ok(())
    }

    fn validate_itinerary(&self, itinerary: &[ItineraryEntry]) -> Result<(), IxaError> {
        let mut setting_counts: HashMap<TypeId, HashSet<usize>> = HashMap::new();
        for itinerary_entry in itinerary {
            let setting_id = itinerary_entry.setting_id;
            let setting_type = itinerary_entry.setting_type;
            if let Some(setting_count_set) = setting_counts.get(&setting_type) {
                if setting_count_set.contains(&setting_id) {
                    return Err(IxaError::from("Duplicated setting".to_string()));
                }
            }
            setting_counts
                .entry(setting_type)
                .or_default()
                .insert(setting_id);

            let setting_ratio = itinerary_entry.ratio;
            if setting_ratio < 0.0 {
                return Err(IxaError::from(
                    "Setting ratio must be greater than or equal to 0".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn get_setting_members<T: SettingType + 'static>(
        &self,
        setting_id: SettingId<T>,
    ) -> Option<&Vec<PersonId>> {
        self.get_data_container(SettingDataPlugin)?
            .get_setting_members(&TypeId::of::<T>(), setting_id.id)
    }

    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut collector = 0.0;
        container.with_itinerary(person_id, |setting_type, setting_props, members, ratio| {
            let multiplier = setting_type.calculate_multiplier(members, *setting_props);
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

    fn get_contact<T: SettingType + 'static, Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
        q: Q,
    ) -> Option<PersonId> {
        self.get_contact_internal(person_id, TypeId::of::<T>(), setting_id.id, q)
    }
    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Option<PersonId> {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut itinerary_multiplier = Vec::new();
        container.with_itinerary(person_id, |setting_type, setting_props, members, ratio| {
            let multiplier = setting_type.calculate_multiplier(members, *setting_props);
            itinerary_multiplier.push(ratio * multiplier);
        });

        let setting_index = self.sample_weighted(SettingsRng, &itinerary_multiplier);

        if let Some(itinerary) = self.get_itinerary(person_id) {
            let itinerary_entry = &itinerary[setting_index];
            self.get_contact_internal(
                person_id,
                itinerary_entry.setting_type,
                itinerary_entry.setting_id,
                q,
            )
        } else {
            None
        }
    }
}

pub fn init(context: &mut Context) {
    let Params {
        settings_properties,
        ..
    } = context.get_params();

    for setting in &settings_properties.clone() {
        match setting {
            CoreSettingsTypes::Home { alpha } => {
                context.register_setting_type(Home {}, SettingProperties { alpha: *alpha });
            }
            CoreSettingsTypes::CensusTract { alpha } => {
                context.register_setting_type(CensusTract {}, SettingProperties { alpha: *alpha });
            }
            CoreSettingsTypes::School { alpha } => {
                context.register_setting_type(School {}, SettingProperties { alpha: *alpha });
            }
            CoreSettingsTypes::Workplace { alpha } => {
                context.register_setting_type(Workplace {}, SettingProperties { alpha: *alpha });
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;
    use crate::{
        parameters::{GlobalParams, RateFnType},
        settings::ContextSettingExt,
    };
    use ixa::{define_person_property, ContextGlobalPropertiesExt, ContextPeopleExt};
    use statrs::assert_almost_eq;

    impl ItineraryEntry {
        pub fn new<T: SettingType>(setting_id: &SettingId<T>, ratio: f64) -> ItineraryEntry {
            ItineraryEntry {
                setting_type: TypeId::of::<T>(),
                setting_id: setting_id.id,
                ratio,
            }
        }
    }

    #[test]
    fn test_setting_type_creation() {
        let mut context = Context::new();
        context
            .register_setting_type(Home {}, SettingProperties { alpha: 0.1 })
            .unwrap();
        context
            .register_setting_type(CensusTract {}, SettingProperties { alpha: 0.001 })
            .unwrap();
        let home_props = context.get_setting_properties::<Home>().unwrap();
        let tract_props = context.get_setting_properties::<CensusTract>().unwrap();

        assert_almost_eq!(0.1, home_props.alpha, 0.0);
        assert_almost_eq!(0.001, tract_props.alpha, 0.0);
    }

    #[test]
    fn test_get_properties_after_registration() {
        let mut context = Context::new();
        context
            .register_setting_type(Home {}, SettingProperties { alpha: 0.1 })
            .unwrap();
        let _ = context.get_setting_properties::<Home>().unwrap();
        let e = context.get_setting_properties::<CensusTract>().err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Attempting to get properties of unregistered setting type");
            }
            Some(ue) => panic!(
                "Expected an error attempting to get properties of unregistered setting type. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_duplicate_setting_type_registration() {
        let mut context = Context::new();
        context
            .register_setting_type(Home {}, SettingProperties { alpha: 0.1 })
            .unwrap();
        let e = context
            .register_setting_type(Home {}, SettingProperties { alpha: 0.001 })
            .err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Setting type is already registered");
            }
            Some(ue) => panic!(
                "Expected an error that there are duplicate settings types. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_duplicated_itinerary() {
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(2), 0.5),
            ItineraryEntry::new(&SettingId::<Home>::new(2), 0.5),
        ];
        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Duplicated setting");
            }
            Some(ue) => panic!(
                "Expected an error that there are duplicate settings. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_feasible_itinerary_ratio() {
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(1), -0.5)];

        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Setting ratio must be greater than or equal to 0");
            }
            Some(ue) => panic!("Expected an error setting ratios should be greater than or equal to 0. Instead got: {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_feasible_itinerary_setting() {
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(&SettingId::<CensusTract>::new(1), 0.5)];

        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Itinerary entry setting type not registered");
            }
            Some(ue) => panic!(
                "Expected an error setting . Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_add_itinerary() {
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 1.0 });

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(1), 0.5),
            ItineraryEntry::new(&SettingId::<Home>::new(2), 0.5),
        ];
        let _ = context.add_itinerary(person, itinerary);
        let members = context
            .get_setting_members::<Home>(SettingId::<Home>::new(2))
            .unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_person(()).unwrap();
        let itinerary2 = vec![ItineraryEntry::new(&SettingId::<Home>::new(2), 1.0)];
        let _ = context.add_itinerary(person2, itinerary2);

        let members2 = context
            .get_setting_members::<Home>(SettingId::<Home>::new(2))
            .unwrap();
        assert_eq!(members2.len(), 2);
    }

    #[test]
    fn test_setting_registration() {
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        let _ = context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(&SettingId::<Home>::new(s), 0.5),
                    ItineraryEntry::new(&SettingId::<CensusTract>::new(s), 0.5),
                ];
                let _ = context.add_itinerary(person, itinerary);
            }
            let members = context
                .get_setting_members::<Home>(SettingId::<Home>::new(s))
                .unwrap();
            let tract_members = context
                .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(s))
                .unwrap();
            // Get the number of people for these settings and should be 5
            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_multiplier() {
        // TODO: if setting not registered, shouldn't be able to register people to setting
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(s), 0.5)];
                let _ = context.add_itinerary(person, itinerary);
            }
        }

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(home_id), 0.5)];
        let _ = context.add_itinerary(person, itinerary);
        let members = context
            .get_setting_members::<Home>(SettingId::<Home>::new(home_id))
            .unwrap();

        let setting_type = Home {};

        let inf_multiplier =
            setting_type.calculate_multiplier(members, SettingProperties { alpha: 0.1 });

        // This is assuming we know what the function for Home is (N - 1) ^ alpha
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);
    }

    #[test]
    fn test_total_infectiousness_multiplier() {
        // Go through all the settings and compute infectiousness multiplier
        let mut context = Context::new();
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        let _ = context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });

        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(&SettingId::<Home>::new(s), 0.5),
                    ItineraryEntry::new(&SettingId::<CensusTract>::new(s), 0.5),
                ];
                let _ = context.add_itinerary(person, itinerary);
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0)];
        let person = context.add_person(()).unwrap();
        let _ = context.add_itinerary(person, itinerary);

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context.calculate_total_infectiousness_multiplier_for_person(person);
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);

        // If person's itinerary is changed for two settings,
        // CensusTract 0 should have 6 members, Home 0 should have 7 members
        // the total infectiousness should be the sum of infs * proportion
        let person = context.add_person(()).unwrap();
        let itinerary_complete = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(0), 0.5),
            ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.5),
        ];
        let _ = context.add_itinerary(person, itinerary_complete);
        let members_home = context
            .get_setting_members::<Home>(SettingId::<Home>::new(0))
            .unwrap();
        let members_tract = context
            .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(0))
            .unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings =
            context.calculate_total_infectiousness_multiplier_for_person(person);

        assert_almost_eq!(
            inf_multiplier_two_settings,
            (f64::from(7 - 1).powf(0.1)) * 0.5 + (f64::from(6 - 1).powf(0.01)) * 0.5,
            0.0
        );
    }

    #[test]
    fn test_get_contact_from_setting() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        // TODO: What happens if the person isn't registered in the setting?
        let mut context = Context::new();
        context.init_random(42);
        let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        let _ = context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(0), 0.5),
            ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0)];
        let _ = context.add_itinerary(person_a, itinerary_a);
        let _ = context.add_itinerary(person_b, itinerary_b);

        assert_eq!(
            person_b,
            context
                .get_contact(person_a, SettingId::<Home>::new(0), ())
                .unwrap()
        );
        assert!(context
            .get_contact(person_a, SettingId::<CensusTract>::new(0), ())
            .is_none());

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.5)];
        let _ = context.add_itinerary(person_c, itinerary_c);

        assert!(context
            .get_contact::<CensusTract>(person_b, SettingId::<CensusTract>::new(0))
            .is_none());

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.5)];
        let _ = context.add_itinerary(person_c, itinerary_c);

        assert!(context
            .get_contact::<CensusTract>(person_b, SettingId::<CensusTract>::new(0))
            .is_none());
    }

    #[test]
    fn test_draw_contact_from_transmitter_itinerary() {
        /*
        Run 100 times
        - Create 3 people at home, and 3 people at censustract
        - Create 7th person with itinerary at home and census tract
        - Call "draw contact from itinerary":
          + Compute total infectiousness
          + Draw a setting weighted by total infectiousness
          + Sample contact from chosen setting
         - Test 1 Itinerary with 0 proportion at census tract, contacts drawn should be from home (0-2)
         - Test 2 Itinerary with 0 proportion at home, contacts should be drawn from census tract (3-6)
         */
        for seed in 0..100 {
            let mut context = Context::new();
            context.init_random(seed);
            let _ = context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
            let _ =
                context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });

            for _ in 0..3 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0)];
                let _ = context.add_itinerary(person, itinerary);
            }

            for _ in 0..3 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 1.0)];
                let _ = context.add_itinerary(person, itinerary);
            }

            let person = context.add_person(()).unwrap();
            let itinerary_home = vec![
                ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0),
                ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.0),
            ];
            let itinerary_censustract = vec![
                ItineraryEntry::new(&SettingId::<Home>::new(0), 0.0),
                ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 1.0),
            ];
            let home_members = context
                .get_setting_members::<Home>(SettingId::<Home>::new(0))
                .unwrap()
                .clone();
            let tract_members = context
                .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(0))
                .unwrap()
                .clone();

            let _ = context.add_itinerary(person, itinerary_home);
            let contact_id_home = context.draw_contact_from_transmitter_itinerary(person, ());
            assert!(home_members.contains(&contact_id_home.unwrap()));

            let _ = context.add_itinerary(person, itinerary_censustract);
            let contact_id_tract = context.draw_contact_from_transmitter_itinerary(person, ());
            assert!(tract_members.contains(&contact_id_tract.unwrap()));
        }
    }

    define_person_property!(Age, usize);

    #[test]
    fn test_draw_contact_from_transmitter_itinerary_with_query() {
        /*
        Run 100 times
        - Create 3 people at home, and 3 people at censustract
        - Create 7th person with itinerary at home and census tract
        - Assign Age property to people and query for only Age = 42
        - Call "draw contact from itinerary":
          + Compute total infectiousness
          + Draw a setting weighted by total infectiousness
          + Sample contact from chosen setting
         - Test 1 Itinerary with 0 proportion at census tract, contacts drawn should be from home (0-2)
         - Test 2 Itinerary with 0 proportion at home, contacts should be drawn from census tract (3-6)
         */
        let mut context = Context::new();
        context.init_random(1234);
        context.register_setting_type(Home {}, SettingProperties { alpha: 0.1 });
        context.register_setting_type(CensusTract {}, SettingProperties { alpha: 0.01 });

        for i in 0..3 {
            let person = context.add_person((Age, 42 + i)).unwrap();
            let itinerary = vec![ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0)];
            let _ = context.add_itinerary(person, itinerary);
        }

        for i in 3..6 {
            let person = context.add_person((Age, 39 + i)).unwrap();
            let itinerary = vec![ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 1.0)];
            let _ = context.add_itinerary(person, itinerary);
        }

        let person = context.add_person((Age, 42)).unwrap();
        let itinerary_home = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(0), 1.0),
            ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 0.0),
        ];
        let itinerary_censustract = vec![
            ItineraryEntry::new(&SettingId::<Home>::new(0), 0.0),
            ItineraryEntry::new(&SettingId::<CensusTract>::new(0), 1.0),
        ];
        let home_members = context
            .get_setting_members::<Home>(SettingId::<Home>::new(0))
            .unwrap()
            .clone();
        let tract_members = context
            .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(0))
            .unwrap()
            .clone();

        let _ = context.add_itinerary(person, itinerary_home);
        let contact_id_home = context.draw_contact_from_transmitter_itinerary(person, (Age, 42));
        assert!(home_members.contains(&contact_id_home.unwrap()));
        assert_eq!(
            context.get_person_property(contact_id_home.unwrap(), Age),
            42
        );

        let _ = context.add_itinerary(person, itinerary_censustract);
        let contact_id_tract = context.draw_contact_from_transmitter_itinerary(person, (Age, 42));
        assert!(tract_members.contains(&contact_id_tract.unwrap()));
        assert_eq!(
            context.get_person_property(contact_id_tract.unwrap(), Age),
            42
        );
    }

    define_setting_type!(HomogeneousMixing);

    #[test]
    fn test_setting_split() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![
                CoreSettingsTypes::Home { alpha: 0.0 },
                CoreSettingsTypes::CensusTract { alpha: 0.0 },
                CoreSettingsTypes::School { alpha: 0.0 },
                CoreSettingsTypes::Workplace { alpha: 0.0 },
            ],
            itinerary_fn_type: ItineraryWriteFnType::Split {
                home: 0.2,
                school: 0.25,
                workplace: 0.5,
                census_tract: 0.52,
            },
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let itinerary_writer = context.get_itinerary_write_rules();
        assert_eq!(
            itinerary_writer(&context, TypeId::of::<Home>(), 1).unwrap(),
            ItineraryEntry {
                setting_type: TypeId::of::<Home>(),
                setting_id: 1,
                ratio: 0.2,
            }
        );

        assert_eq!(
            itinerary_writer(&context, TypeId::of::<School>(), 2).unwrap(),
            ItineraryEntry {
                setting_type: TypeId::of::<School>(),
                setting_id: 2,
                ratio: 0.25,
            }
        );

        assert_eq!(
            itinerary_writer(&context, TypeId::of::<Workplace>(), 2).unwrap(),
            ItineraryEntry {
                setting_type: TypeId::of::<Workplace>(),
                setting_id: 2,
                ratio: 0.5,
            }
        );

        assert_eq!(
            itinerary_writer(&context, TypeId::of::<CensusTract>(), 1).unwrap(),
            ItineraryEntry {
                setting_type: TypeId::of::<CensusTract>(),
                setting_id: 1,
                ratio: 0.52,
            }
        );

        let e = itinerary_writer(&context, TypeId::of::<HomogeneousMixing>(), 1).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The `Split` itinerary write method only supports ratios in core setting types.
                            A non core setting type was provided.");
            }
            Some(ue) => panic!(
                "Expected an error that itinerary write rules do not support this setting type. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_setting_split_evenly() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let itinerary_writer = context.get_itinerary_write_rules();
        assert_eq!(
            itinerary_writer(&context, TypeId::of::<Home>(), 1).unwrap(),
            ItineraryEntry {
                setting_type: TypeId::of::<Home>(),
                setting_id: 1,
                ratio: 1.0,
            }
        );
    }
    /*TODO:
    Test failure of getting properties if not initialized
    Test failure if a setting is registered more than once?
    Test that proportions either add to 1 or that they are weighted based on proportion
    */
}
