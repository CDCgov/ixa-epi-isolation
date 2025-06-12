use crate::parameters::{
    ContextParametersExt, CoreSettingsTypes, ItinerarySpecificationType, Params,
};
use ixa::{
    define_data_plugin, define_rng, people::Query, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId,
};
use serde::{Deserialize, Serialize};

use std::{
    any::TypeId,
    collections::{hash_map::Entry, HashMap, HashSet},
    hash::Hash,
};

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
    pub itinerary_specification: Option<ItinerarySpecificationType>,
}

pub trait SettingType {
    fn calculate_multiplier(
        &self,
        members: &[PersonId],
        setting_properties: SettingProperties,
    ) -> f64;
    fn get_type_id(&self) -> TypeId;
    fn get_name(&self) -> &'static str;
}

#[derive(Debug, PartialEq)]
pub struct SettingId<'a, T: SettingType + ?Sized> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    pub setting_type: &'a T,
}

impl<'a, T: SettingType + ?Sized> SettingId<'a, T> {
    pub fn new(setting_type: &'a T, id: usize) -> SettingId<'a, T> {
        SettingId { id, setting_type }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ItineraryEntry {
    setting_type: TypeId,
    setting_id: usize,
    ratio: f64,
}

impl ItineraryEntry {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new<T: SettingType + ?Sized>(setting_id: SettingId<T>, ratio: f64) -> ItineraryEntry {
        ItineraryEntry {
            setting_type: setting_id.setting_type.get_type_id(),
            setting_id: setting_id.id,
            ratio,
        }
    }
}

#[allow(dead_code)]
pub enum ItineraryModifiers<'a> {
    // Replace itinerary with a new vector of itinerary entries
    ReplaceWith { itinerary: Vec<ItineraryEntry> },
    // Reduce the current itinerary to a setting type (e.g., Home)
    RestrictTo { setting: &'a dyn SettingType },
    // Exclude setting types from current itinerary (e.g., Workplace)
    Exclude { setting: &'a dyn SettingType },
}

pub fn append_itinerary_entry<T: SettingType + Copy + 'static>(
    itinerary: &mut Vec<ItineraryEntry>,
    context: &Context,
    setting_type: T,
    setting_id: usize,
) -> Result<(), IxaError> {
    // Is this setting type registered? Our population loader is hard coded to always try to put
    // people in the core setting types, but sometimes we don't want all the core setting types
    // (we didn't specify them). So, first check that the setting in question exists.
    if context
        .get_data_container(SettingDataPlugin)
        .ok_or(IxaError::IxaError(
            "Settings must be initialized prior to making itineraries.".to_string(),
        ))?
        .setting_properties
        .contains_key(&TypeId::of::<T>())
    {
        let ratio = get_itinerary_ratio(context, setting_type)?;
        // No point in adding an itinerary entry if the ratio is zero
        if ratio != 0.0 {
            itinerary.push(ItineraryEntry::new(
                SettingId::new(&setting_type, setting_id),
                ratio,
            ));
        }
    }
    Ok(())
}

// In the future, this method could take the person id as an argument for making individual-level
// itineraries.
fn get_itinerary_ratio<T: SettingType + 'static>(
    context: &Context,
    _setting_type: T,
) -> Result<f64, IxaError> {
    let setting_properties = context
        .get_data_container(SettingDataPlugin)
        .unwrap() // We can unwrap here because we would have already propagated an error in the
        // calling code if the settings data container did not exist.
        .setting_properties
        .get(&TypeId::of::<T>())
        .unwrap(); // We can unwrap here because we already checked that this setting type exists

    match setting_properties.itinerary_specification {
        Some(ItinerarySpecificationType::Constant { ratio }) => Ok(ratio),
        None => Err(IxaError::IxaError(
            "Itinerary specification type is None, so ratios must be specified manually."
                .to_string(),
        )),
    }
}

#[derive(Default)]
struct SettingDataContainer {
    setting_types: HashMap<TypeId, Box<dyn SettingType>>,
    // For each setting type (e.g., Home) store the properties (e.g., alpha)
    setting_properties: HashMap<TypeId, SettingProperties>,
    // For each setting type, have a map of each setting id and a list of members
    members: HashMap<TypeId, HashMap<usize, Vec<PersonId>>>,
    itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
    modified_itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
}

impl SettingDataContainer {
    fn get_setting_members(
        &self,
        setting_type: &TypeId,
        setting_id: usize,
    ) -> Option<&Vec<PersonId>> {
        self.members.get(setting_type)?.get(&setting_id)
    }
    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        if let Some(modified_itinerary) = self.modified_itineraries.get(&person_id) {
            return Some(modified_itinerary);
        }

        if let Some(itinerary) = self.itineraries.get(&person_id) {
            return Some(itinerary);
        }
        None
    }
    fn with_itinerary<F>(&self, person_id: PersonId, mut callback: F)
    where
        F: FnMut(&dyn SettingType, &SettingProperties, &Vec<PersonId>, f64),
    {
        if let Some(itinerary) = self.get_itinerary(person_id) {
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
    fn add_member_to_itinerary_setting(
        &mut self,
        person_id: PersonId,
        itinerary: &Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        for itinerary_entry in itinerary {
            // TODO: If we are changing a person's itinerary, the person_id should be removed from vector
            // This isn't the same as the concept of being present or not.
            if !self
                .setting_types
                .contains_key(&itinerary_entry.setting_type)
            {
                return Err(IxaError::from(
                    "Itinerary entry setting type not registered",
                ));
            }
            self.members
                .entry(itinerary_entry.setting_type)
                .or_default()
                .entry(itinerary_entry.setting_id)
                .or_default()
                .push(person_id);
        }
        Ok(())
    }
    fn remove_member_from_itinerary_settings(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) {
        for itinerary_entry in itinerary {
            self.members
                .entry(itinerary_entry.setting_type)
                .or_default()
                .entry(itinerary_entry.setting_id)
                .or_default()
                .retain(|&x| x != person_id);
        }
    }
}

#[macro_export]
macro_rules! define_setting_type {
    ($name:ident) => {
        #[derive(Default, Copy, Clone, Debug, Hash, Eq, PartialEq)]
        pub struct $name;

        impl $crate::settings::SettingType for $name {
            #[allow(clippy::cast_precision_loss)]
            fn calculate_multiplier(
                &self,
                members: &[ixa::PersonId],
                setting_properties: $crate::settings::SettingProperties,
            ) -> f64 {
                let n_members = members.len();
                ((n_members - 1) as f64).powf(setting_properties.alpha)
            }
            fn get_type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<$name>()
            }
            fn get_name(&self) -> &'static str {
                stringify!($name)
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
    SettingDataContainer::default()
);

#[allow(dead_code)]
pub trait ContextSettingExt {
    fn get_setting_properties<T: SettingType + 'static>(
        &self,
        setting_type: T,
    ) -> Result<SettingProperties, IxaError>;
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting: T,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError>;
    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError>;
    fn modify_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary_modifier: ItineraryModifiers,
    ) -> Result<(), IxaError>;
    fn remove_modified_itinerary(&mut self, person_id: PersonId) -> Result<(), IxaError>;
    fn validate_itinerary(&self, itinerary: &[ItineraryEntry]) -> Result<(), IxaError>;

    /// `get_setting_ids` returns a vector of the numerical values of the ID for a setting type
    fn get_setting_ids<T: SettingType>(
        &mut self,
        person_id: PersonId,
        setting_type: T,
    ) -> Vec<usize>;
    fn get_setting_members<T: SettingType + 'static>(
        &self,
        setting_id: SettingId<T>,
    ) -> Option<&Vec<PersonId>>;
    /// Get the total infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of members in the setting
    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64;

    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>>;
    fn get_contact<T: SettingType + ?Sized, Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError>;
    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError>;
    fn get_setting_for_contact(&self, person_id: PersonId) -> Option<SettingId<dyn SettingType>>;
}

trait ContextSettingInternalExt {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting_type: TypeId,
        setting_id: usize,
        q: T,
    ) -> Result<Option<PersonId>, IxaError>;
    fn get_setting_members_internal(
        &self,
        setting_type: TypeId,
        setting_id: usize,
    ) -> Option<&Vec<PersonId>>;
    /// Takes an itinerary and adds makes it the modified itinerary of `person id`
    /// This modified itinerary is used as the person's itinerary instead of default itinerary
    /// for as long as modified itinerary exists in the container.
    fn add_modified_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError>;
    /// Limit the current itinerary to a specified setting type (e.g., Home)
    /// The proportion of the rest of the settings remains unchanged
    fn limit_itinerary_by_setting_type(
        &mut self,
        person_id: PersonId,
        setting_type: &dyn SettingType,
    ) -> Result<(), IxaError>;
    fn exclude_setting_from_itinerary(
        &mut self,
        person_id: PersonId,
        setting_type: &dyn SettingType,
    ) -> Result<(), IxaError>;
}

impl ContextSettingInternalExt for Context {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting_type: TypeId,
        setting_id: usize,
        q: T,
    ) -> Result<Option<PersonId>, IxaError> {
        let members = self.get_setting_members_internal(setting_type, setting_id);
        if let Some(members) = members {
            if !members.contains(&person_id) {
                return Err(IxaError::from(
                    "Attempting contact outside of group membership",
                ));
            }
            // The setting has one person in it -- this person
            if members.len() == 1 {
                return Ok(None);
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
                return Ok(None);
            }

            Ok(Some(
                contacts[self.sample_range(SettingsRng, 0..contacts.len())],
            ))
        } else {
            Err(IxaError::from("Group membership is None"))
        }
    }
    fn get_setting_members_internal(
        &self,
        setting_type: TypeId,
        setting_id: usize,
    ) -> Option<&Vec<PersonId>> {
        self.get_data_container(SettingDataPlugin)?
            .get_setting_members(&setting_type, setting_id)
    }

    fn add_modified_itinerary(
        &mut self,
        person_id: PersonId,
        mut itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        // Normalize itinerary ratios
        self.validate_itinerary(&itinerary)?;

        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        // If we passed validation, we know setting entries aren't all zero, so we can divide by
        // total_ratio without worrying about dividing by zero.
        for entry in &mut itinerary {
            entry.ratio /= total_ratio;
        }
        let container = self.get_data_container_mut(SettingDataPlugin);

        // If there's a modified itinerary present, replace with this
        if container.modified_itineraries.contains_key(&person_id) {
            return Err(IxaError::from(
                 "Can't modify itinerary because a modified itinerary is already present. Remove and add new modified itinerary."
             ));
        }

        // Remove people from default itinerary, if there's one
        match container.itineraries.get(&person_id) {
            None => {
                return Err(IxaError::from(
                    "Can't modify itinerary if there isn't one present",
                ))
            }
            Some(previous_itinerary) => {
                container
                    .remove_member_from_itinerary_settings(person_id, previous_itinerary.clone());
            }
        }

        container.add_member_to_itinerary_setting(person_id, &itinerary)?;
        container.modified_itineraries.insert(person_id, itinerary);

        Ok(())
    }

    fn exclude_setting_from_itinerary(
        &mut self,
        person_id: PersonId,
        setting_type: &dyn SettingType,
    ) -> Result<(), IxaError> {
        let container = self.get_data_container_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Err(IxaError::from("Can't find itinerary for person")),
            Some(itinerary_vector) => {
                let mut modified_itinerary = Vec::<ItineraryEntry>::new();
                for entry in itinerary_vector {
                    if entry.setting_type != setting_type.get_type_id() {
                        modified_itinerary.push(*entry);
                    }
                }
                if modified_itinerary.is_empty() {
                    return Err(IxaError::from(
                        "Exclude itinerary resulted in empty modified itinerary",
                    ));
                }

                self.add_modified_itinerary(person_id, modified_itinerary)?;
                Ok(())
            }
        }
    }
    fn limit_itinerary_by_setting_type(
        &mut self,
        person_id: PersonId,
        setting_type: &dyn SettingType,
    ) -> Result<(), IxaError> {
        let container = self.get_data_container_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Err(IxaError::from("Can't find itinerary for person")),
            Some(itineraries) => {
                let mut modified_itinerary = Vec::<ItineraryEntry>::new();
                for entry in itineraries {
                    if entry.setting_type == setting_type.get_type_id() {
                        modified_itinerary.push(*entry);
                    }
                }
                if modified_itinerary.is_empty() {
                    return Err(IxaError::from(
                        "limit itinerary resulted in empty modified itinerary",
                    ));
                }

                self.add_modified_itinerary(person_id, modified_itinerary)?;
                Ok(())
            }
        }
    }
}

impl ContextSettingExt for Context {
    fn get_setting_properties<T: SettingType + 'static>(
        &self,
        _setting_properties: T,
    ) -> Result<SettingProperties, IxaError> {
        let data_container =
            self.get_data_container(SettingDataPlugin)
                .ok_or(IxaError::IxaError(
                    "Setting plugin data is none".to_string(),
                ))?;

        match data_container.setting_properties.get(&TypeId::of::<T>()) {
            None => Err(IxaError::from(
                "Attempting to get properties of unregistered setting type",
            )),
            Some(properties) => Ok(*properties),
        }
    }
    fn register_setting_type<T: SettingType + 'static>(
        &mut self,
        setting_type: T,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError> {
        let container = self.get_data_container_mut(SettingDataPlugin);

        match container.setting_types.entry(TypeId::of::<T>()) {
            Entry::Vacant(entry) => {
                entry.insert(Box::new(setting_type));
            }
            Entry::Occupied(_) => return Err(IxaError::from("Setting type is already registered")),
        }

        // Add properties
        container
            .setting_properties
            .insert(TypeId::of::<T>(), setting_props);
        Ok(())
    }

    fn remove_modified_itinerary(&mut self, person_id: PersonId) -> Result<(), IxaError> {
        let container = self.get_data_container_mut(SettingDataPlugin);

        // If there's a modified itinerary present, remove
        if let Some(previous_mod_itinerary) = container.modified_itineraries.get(&person_id) {
            container
                .remove_member_from_itinerary_settings(person_id, previous_mod_itinerary.clone());
        }

        container.modified_itineraries.remove(&person_id);

        // Get people back to default itinerary, if there's one
        match container.itineraries.get(&person_id) {
            None => {
                return Err(IxaError::from(
                    "Can't modify itinerary if there isn't one present",
                ))
            }
            Some(default_itinerary) => {
                for itinerary_entry in default_itinerary {
                    if !container
                        .setting_types
                        .contains_key(&itinerary_entry.setting_type)
                    {
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
                }
            }
        }
        Ok(())
    }

    fn modify_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary_modifier: ItineraryModifiers,
    ) -> Result<(), IxaError> {
        match itinerary_modifier {
            ItineraryModifiers::ReplaceWith { itinerary } => {
                trace!("ItineraryModifier::Replace person {person_id} --  {itinerary:?}");
                self.add_modified_itinerary(person_id, itinerary)
            }
            ItineraryModifiers::RestrictTo { setting } => {
                trace!(
                    "ItineraryModifier::RestrictTo person {person_id} -- {:?}",
                    setting.get_type_id()
                );
                self.limit_itinerary_by_setting_type(person_id, setting)
            }
            ItineraryModifiers::Exclude { setting } => {
                trace!(
                    "ItineraryModifier::Exclude person {person_id}-- {:?}",
                    setting.get_type_id()
                );
                self.exclude_setting_from_itinerary(person_id, setting)
            }
        }
    }

    fn get_setting_ids<T: SettingType>(
        &mut self,
        person_id: PersonId,
        setting_type: T,
    ) -> Vec<usize> {
        let container = self.get_data_container_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Vec::new(),
            Some(itineraries) => {
                let mut setting_id_vec = Vec::new();
                for entry in itineraries {
                    if entry.setting_type == setting_type.get_type_id() {
                        setting_id_vec.push(entry.setting_id);
                    }
                }
                setting_id_vec
            }
        }
    }

    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        mut itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        // Normalize itinerary ratios
        self.validate_itinerary(&itinerary)?;

        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        // If we passed validation, we know setting entries aren't all zero, so we can divide by
        // total_ratio without worrying about dividing by zero.
        for entry in &mut itinerary {
            entry.ratio /= total_ratio;
        }
        let container = self.get_data_container_mut(SettingDataPlugin);

        // Clean up settings that from previous itinerary, if there is one

        if let Some(previous_itinerary) = container.itineraries.get(&person_id) {
            container.remove_member_from_itinerary_settings(person_id, previous_itinerary.clone());
        }

        container.add_member_to_itinerary_setting(person_id, &itinerary)?;
        container.itineraries.insert(person_id, itinerary);

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
            .get_itinerary(person_id)
    }

    fn get_contact<T: SettingType + ?Sized, Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting_id: SettingId<T>,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError> {
        // let container: &SettingDataContainer = self.get_data_container(SettingDataPlugin).unwrap();
        self.get_contact_internal(
            person_id,
            setting_id.setting_type.get_type_id(),
            setting_id.id,
            q,
        )
    }
    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError> {
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
            Ok(None)
        }
    }
    fn get_setting_for_contact(&self, person_id: PersonId) -> Option<SettingId<dyn SettingType>> {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut itinerary_multiplier = Vec::new();
        container.with_itinerary(person_id, |setting_type, setting_props, members, ratio| {
            let multiplier = setting_type.calculate_multiplier(members, *setting_props);
            itinerary_multiplier.push(ratio * multiplier);
        });

        let setting_index = self.sample_weighted(SettingsRng, &itinerary_multiplier);

        if let Some(itinerary) = self.get_itinerary(person_id) {
            let itinerary_entry = &itinerary[setting_index];
            let setting_type = container
                .setting_types
                .get(&itinerary_entry.setting_type)
                .unwrap()
                .as_ref();
            Some(SettingId::new(setting_type, itinerary_entry.setting_id))
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

    for (setting_type, setting_properties) in settings_properties.clone() {
        match setting_type {
            CoreSettingsTypes::Home => {
                context
                    .register_setting_type(Home, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::CensusTract => {
                context
                    .register_setting_type(CensusTract, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::School => {
                context
                    .register_setting_type(School, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::Workplace => {
                context
                    .register_setting_type(Workplace, setting_properties)
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod test {

    use std::path::PathBuf;

    use super::*;
    use crate::{
        parameters::{GlobalParams, ItinerarySpecificationType, RateFnType},
        settings::ContextSettingExt,
    };
    use ixa::{define_person_property, ContextGlobalPropertiesExt, ContextPeopleExt};
    use statrs::assert_almost_eq;

    define_setting_type!(Community);

    fn register_default_settings(context: &mut Context) {
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                Workplace,
                SettingProperties {
                    alpha: 0.3,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        context
            .register_setting_type(
                School,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();
    }

    #[test]
    fn test_setting_type_creation() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.001,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.25,
                    }),
                },
            )
            .unwrap();
        let home_props = context.get_setting_properties(Home).unwrap();
        let tract_props = context.get_setting_properties(CensusTract).unwrap();

        assert_almost_eq!(0.1, home_props.alpha, 0.0);
        assert_eq!(
            ItinerarySpecificationType::Constant { ratio: 0.5 },
            home_props.itinerary_specification.unwrap()
        );
        assert_almost_eq!(0.001, tract_props.alpha, 0.0);
        assert_eq!(
            ItinerarySpecificationType::Constant { ratio: 0.25 },
            tract_props.itinerary_specification.unwrap()
        );
    }

    #[test]
    fn test_get_properties_after_registration() {
        let mut context = Context::new();
        let e = context.get_setting_properties(Home).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Setting plugin data is none");
            }
            Some(ue) => panic!(
                "Expected an error setting plugin data is none. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        context
            .register_setting_type(
                Home {},
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context.get_setting_properties(Home).unwrap();
        let e = context.get_setting_properties(CensusTract).err();
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
            .register_setting_type(
                Home {},
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        let e = context
            .register_setting_type(
                Home {},
                SettingProperties {
                    alpha: 0.001,
                    itinerary_specification: None,
                },
            )
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
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 2), 0.5),
            ItineraryEntry::new(SettingId::new(&Home, 2), 0.5),
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
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, 1), -0.5)];

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
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();

        // Community is a defined setting but not registered
        let itinerary = vec![ItineraryEntry::new(SettingId::new(&Community, 1), 0.5)];

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
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 1), 0.5),
            ItineraryEntry::new(SettingId::new(&Home, 2), 0.5),
        ];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(SettingId::new(&Home, 2))
            .unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_person(()).unwrap();
        let itinerary2 = vec![ItineraryEntry::new(SettingId::new(&Home, 2), 1.0)];
        context.add_itinerary(person2, itinerary2).unwrap();

        let members2 = context
            .get_setting_members(SettingId::new(&Home, 2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let members2 = context
            .get_setting_members(SettingId::new(&Home, 2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let itinerary3 = vec![ItineraryEntry::new(SettingId::new(&Home, 3), 0.5)];
        context.add_itinerary(person, itinerary3).unwrap();
        let members2_removed = context
            .get_setting_members(SettingId::new(&Home, 2))
            .unwrap();
        assert_eq!(members2_removed.len(), 1);
        let members3 = context
            .get_setting_members(SettingId::new(&Home, 3))
            .unwrap();
        assert_eq!(members3.len(), 1);
        let members1_removed = context
            .get_setting_members(SettingId::new(&Home, 1))
            .unwrap();
        assert_eq!(members1_removed.len(), 0);
    }

    #[test]
    fn test_get_setting_ids() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let person_two = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 0.5),
        ];

        let itinerary_two = vec![ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0)];

        context.add_itinerary(person, itinerary).unwrap();
        context.add_itinerary(person_two, itinerary_two).unwrap();

        let h_id = context.get_setting_ids(person, Home);
        let w_id = context.get_setting_ids(person_two, Workplace);

        let h_members = context
            .get_setting_members(SettingId::new(&Home, h_id[0]))
            .unwrap();
        let w_members = context
            .get_setting_members(SettingId::new(&Workplace, w_id[0]))
            .unwrap();

        assert_eq!(h_members.len(), 1);
        assert_eq!(w_members.len(), 2);
    }

    #[test]
    fn test_itinerary_modifier_enum() {
        let mut context = Context::new();
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0),
        ];
        let isolation_itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];

        let _ = context.add_itinerary(person, itinerary);

        let members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 1);

        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::ReplaceWith {
                itinerary: isolation_itinerary,
            },
        );

        let members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 0);
    }

    #[test]
    fn test_itinerary_modifiers_replace() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let itinerary_vec: Vec<Vec<(&dyn SettingType, usize)>> = vec![
            vec![(&Home, 0), (&Workplace, 0), (&School, 0)],
            vec![(&Home, 0), (&Workplace, 0)],
            vec![(&Home, 0), (&Workplace, 0)],
            vec![(&Home, 1), (&School, 0)],
            vec![(&Home, 1), (&Workplace, 0)],
            vec![(&Home, 1), (&Workplace, 0)],
        ];

        let mut person_0: Option<PersonId> = None;
        for (p_id, p_it) in itinerary_vec.iter().enumerate() {
            let mut p_itinerary = Vec::<ItineraryEntry>::new();
            for (s_type, s_id) in p_it.clone() {
                p_itinerary.push(ItineraryEntry::new(SettingId::new(s_type, s_id), 1.0));
            }
            let person = context.add_person(()).unwrap();
            let _e = context.add_itinerary(person, p_itinerary);
            if p_id == 0 {
                person_0 = Some(person);
            }
        }
        let alpha_h = context.get_setting_properties(Home).unwrap().alpha;
        let alpha_w = context.get_setting_properties(Workplace).unwrap().alpha;
        let alpha_s = context.get_setting_properties(School).unwrap().alpha;

        let inf_multiplier =
            context.calculate_total_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = (1.0 / 3.0) * (2_f64).powf(alpha_h)
            + (1.0 / 3.0) * (4_f64).powf(alpha_w)
            + (1.0 / 3.0) * (1_f64).powf(alpha_s);

        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.0);

        // 2. Isolate person with itinerary [(Home 0 , 0.95), (Workplace 0, 0.05)]
        let isolation_itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.95),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 0.05),
        ];

        let _ = context.modify_itinerary(
            person_0.unwrap(),
            ItineraryModifiers::ReplaceWith {
                itinerary: isolation_itinerary,
            },
        );

        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let h_one_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 1))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        let s_members = context
            .get_setting_members::<School>(SettingId::new(&School, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(h_one_members.len(), 3);
        assert_eq!(w_members.len(), 5);
        assert_eq!(s_members.len(), 1);

        let inf_multiplier =
            context.calculate_total_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = (0.95) * (2_f64).powf(alpha_h) + (0.05) * (4_f64).powf(alpha_w);
        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.001);

        // 3. Remove modified itinerary; get back to normal
        let _ = context.remove_modified_itinerary(person_0.unwrap());
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let h_one_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 1))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        let s_members = context
            .get_setting_members::<School>(SettingId::new(&School, 0))
            .unwrap();

        assert_eq!(h_members.len(), 3);
        assert_eq!(h_one_members.len(), 3);
        assert_eq!(w_members.len(), 5);
        assert_eq!(s_members.len(), 2);

        let inf_multiplier =
            context.calculate_total_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = (1.0 / 3.0) * (2_f64).powf(alpha_h)
            + (1.0 / 3.0) * (4_f64).powf(alpha_w)
            + (1.0 / 3.0) * (1_f64).powf(alpha_s);

        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.001);
    }

    #[test]
    fn test_limited_itinerary_modifier() {
        /* H(0) = [0, 1, 2]
          W(0) = [0,3,4] -
         Person 0 isolates using limited itinerary by setting type.
        */
        let mut context = Context::new();
        context.init_random(42);
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (limit default): {h_members:?}");
        println!("WORK MEMBERS (limit default): {w_members:?}");

        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(person, ItineraryModifiers::RestrictTo { setting: &Home });

        // Check membership
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (limit isolation): {h_members:?}");
        println!("WORK MEMBERS (limit isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary(person);
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);

        println!("HOME MEMBERS (limit isolation): {h_members:?}");
        println!("WORK MEMBERS (limit isolation): {w_members:?}");
    }

    #[test]
    fn test_exclude_setting_from_itinerary() {
        /* H(0) = [0, 1, 2]
          W(0) = [0,3,4] -
         Person 0 isolates by excluding workplace from itinerary by setting type.
        */
        let mut context = Context::new();
        context.init_random(42);
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(SettingId::new(&Workplace, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (exclude default): {h_members:?}");
        println!("WORK MEMBERS (exclude default): {w_members:?}");

        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::Exclude {
                setting: &Workplace,
            },
        );

        // Check membership
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (exclude isolation): {h_members:?}");
        println!("WORK MEMBERS (exclude isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary(person);
        let h_members = context
            .get_setting_members::<Home>(SettingId::new(&Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members::<Workplace>(SettingId::new(&Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);

        println!("HOME MEMBERS (exclude post-isolation): {h_members:?}");
        println!("WORK MEMBERS (exclude post-isolation): {w_members:?}");
    }

    #[test]
    fn test_setting_registration() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::new(&Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(&CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let members = context
                .get_setting_members(SettingId::new(&Home, s))
                .unwrap();
            let tract_members = context
                .get_setting_members(SettingId::new(&CensusTract, s))
                .unwrap();

            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_multiplier() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, s), 0.5)];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, home_id), 0.5)];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(SettingId::new(&Home, home_id))
            .unwrap();

        let setting_type = Home;

        let inf_multiplier = setting_type.calculate_multiplier(
            members,
            SettingProperties {
                alpha: 0.1,
                itinerary_specification: None,
            },
        );

        // This is assuming we know what the function for Home is (N - 1) ^ alpha
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);
    }

    #[test]
    fn test_total_infectiousness_multiplier() {
        // Go through all the settings and compute infectiousness multiplier
        let mut context = Context::new();
        register_default_settings(&mut context);

        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::new(&Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(&CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];
        let person = context.add_person(()).unwrap();
        context.add_itinerary(person, itinerary).unwrap();

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context.calculate_total_infectiousness_multiplier_for_person(person);
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);

        // If person's itinerary is changed for two settings,
        // CensusTract 0 should have 6 members, Home 0 should have 7 members
        // the total infectiousness should be the sum of infs * proportion
        let person = context.add_person(()).unwrap();
        let itinerary_complete = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
        ];
        context.add_itinerary(person, itinerary_complete).unwrap();
        let members_home = context
            .get_setting_members(SettingId::new(&Home, 0))
            .unwrap();
        let members_tract = context
            .get_setting_members(SettingId::new(&CensusTract, 0))
            .unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings =
            context.calculate_total_infectiousness_multiplier_for_person(person);

        let alpha_h = context.get_setting_properties(Home).unwrap().alpha;
        let alpha_ct = context.get_setting_properties(CensusTract).unwrap().alpha;

        assert_almost_eq!(
            inf_multiplier_two_settings,
            (f64::from(7 - 1).powf(alpha_h)) * 0.5 + (f64::from(6 - 1).powf(alpha_ct)) * 0.5,
            0.0
        );
    }

    #[test]
    fn test_get_setting_for_contact() {
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();

        // When person a is used to select a setting for contact, it should return Home. While they are
        // also a member of CensusTract, since they are the only member the multiplier used to weight the
        // selection is 0.0 from calculate_multiplier. Thus the probability CensusTract is selected is 0.0.
        let setting_id = context.get_setting_for_contact(person_a).unwrap();
        assert_eq!(setting_id.setting_type.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id, 0);

        let setting_id = context.get_setting_for_contact(person_b).unwrap();
        assert_eq!(setting_id.setting_type.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id, 0);

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();
        let setting_id = context.get_setting_for_contact(person_c).unwrap();
        assert_eq!(
            setting_id.setting_type.get_type_id(),
            TypeId::of::<CensusTract>()
        );
        assert_eq!(setting_id.id, 0);
    }

    #[test]
    fn test_get_contact_from_setting() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();
        let setting_id = context.get_setting_for_contact(person_a).unwrap();
        assert_eq!(
            Some(person_b),
            context.get_contact(person_a, setting_id, ()).unwrap()
        );

        assert!(context
            .get_contact(person_a, SettingId::new(&CensusTract, 0), ())
            .unwrap()
            .is_none());

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();

        let e = context
            .get_contact(person_b, SettingId::new(&CensusTract, 0), ())
            .err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Attempting contact outside of group membership");
            }
            Some(ue) => panic!(
                "Expected an error attempting contact outside group membership. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        let e = context.get_contact(person_b, SettingId::new(&CensusTract, 10), ());
        match e {
            Err(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Group membership is None");
            }
            Err(ue) => panic!(
                "Expected an error attempting contact outside group membership. Instead got: {:?}",
                ue.to_string()
            ),
            Ok(_) => panic!("Expected an error. Instead, validation passed with no errors."),
        }
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
            context
                .register_setting_type(
                    Home,
                    SettingProperties {
                        alpha: 0.1,
                        itinerary_specification: None,
                    },
                )
                .unwrap();
            context
                .register_setting_type(
                    CensusTract,
                    SettingProperties {
                        alpha: 0.01,
                        itinerary_specification: None,
                    },
                )
                .unwrap();

            for _ in 0..3 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];
                context.add_itinerary(person, itinerary).unwrap();
            }

            for _ in 0..3 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0)];
                context.add_itinerary(person, itinerary).unwrap();
            }

            let person = context.add_person(()).unwrap();
            let itinerary_home = vec![
                ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.0),
            ];
            let itinerary_censustract = vec![
                ItineraryEntry::new(SettingId::new(&Home, 0), 0.0),
                ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0),
            ];
            let home_members = context
                .get_setting_members(SettingId::new(&Home, 0))
                .unwrap()
                .clone();
            let tract_members = context
                .get_setting_members(SettingId::new(&CensusTract, 0))
                .unwrap()
                .clone();

            context.add_itinerary(person, itinerary_home).unwrap();
            let contact_id_home = context.draw_contact_from_transmitter_itinerary(person, ());
            assert!(home_members.contains(&contact_id_home.unwrap().unwrap()));

            context
                .add_itinerary(person, itinerary_censustract)
                .unwrap();
            let contact_id_tract = context.draw_contact_from_transmitter_itinerary(person, ());
            assert!(tract_members.contains(&contact_id_tract.unwrap().unwrap()));
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
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        for i in 0..3 {
            let person = context.add_person((Age, 42 + i)).unwrap();
            let itinerary = vec![ItineraryEntry::new(SettingId::new(&Home, 0), 1.0)];
            context.add_itinerary(person, itinerary).unwrap();
        }

        for i in 3..6 {
            let person = context.add_person((Age, 39 + i)).unwrap();
            let itinerary = vec![ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0)];
            context.add_itinerary(person, itinerary).unwrap();
        }

        let person = context.add_person((Age, 42)).unwrap();
        let itinerary_home = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 0.0),
        ];
        let itinerary_censustract = vec![
            ItineraryEntry::new(SettingId::new(&Home, 0), 0.0),
            ItineraryEntry::new(SettingId::new(&CensusTract, 0), 1.0),
        ];
        let home_members = context
            .get_setting_members(SettingId::new(&Home, 0))
            .unwrap()
            .clone();
        let tract_members = context
            .get_setting_members(SettingId::new(&CensusTract, 0))
            .unwrap()
            .clone();

        context.add_itinerary(person, itinerary_home).unwrap();
        let contact_id_home = context
            .draw_contact_from_transmitter_itinerary(person, (Age, 42))
            .unwrap();
        assert!(home_members.contains(&contact_id_home.unwrap()));
        assert_eq!(
            context.get_person_property(contact_id_home.unwrap(), Age),
            42
        );

        context
            .add_itinerary(person, itinerary_censustract)
            .unwrap();
        let contact_id_tract = context
            .draw_contact_from_transmitter_itinerary(person, (Age, 42))
            .unwrap();
        assert!(tract_members.contains(&contact_id_tract.unwrap()));
        assert_eq!(
            context.get_person_property(contact_id_tract.unwrap(), Age),
            42
        );
    }

    #[test]
    fn test_itinerary_specification_none() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        let e = get_itinerary_ratio(&context, Home).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Itinerary specification type is None, so ratios must be specified manually."
                );
            }
            Some(ue) => panic!(
                "Expected an error that itinerary specification is None. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_append_itinerary_entry() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_type(
                School,
                SettingProperties {
                    alpha: 0.2,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.25,
                    }),
                },
            )
            .unwrap();
        let mut itinerary = vec![];

        // Test appending a valid entry
        append_itinerary_entry(&mut itinerary, &context, Home, 1).unwrap();
        assert_eq!(itinerary.len(), 1);
        assert_eq!(itinerary[0].setting_type, TypeId::of::<Home>());
        assert_eq!(itinerary[0].setting_id, 1);
        assert_almost_eq!(itinerary[0].ratio, 0.5, 0.0);

        // Test appending an entry with a different setting type
        append_itinerary_entry(&mut itinerary, &context, School, 42).unwrap();
        assert_eq!(itinerary.len(), 2);
        assert_eq!(itinerary[1].setting_type, TypeId::of::<School>());
        assert_eq!(itinerary[1].setting_id, 42);
    }

    #[test]
    fn test_get_itinerary_ratio() {
        let mut context = Context::new();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();

        // Test with a valid setting type
        let ratio = get_itinerary_ratio(&context, Home).unwrap();
        assert_almost_eq!(ratio, 0.5, 0.0);
    }

    #[test]
    fn test_only_include_registered_settings_in_itineraries() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::from([(
                CoreSettingsTypes::Home,
                SettingProperties {
                    alpha: 0.5,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )]),
        };

        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        init(&mut context);
        let mut iitinerary = vec![];
        append_itinerary_entry(&mut iitinerary, &context, Workplace, 1).unwrap();

        assert_eq!(iitinerary.len(), 0);

        append_itinerary_entry(&mut iitinerary, &context, Home, 1).unwrap();
        assert_eq!(iitinerary.len(), 1);
        assert_eq!(iitinerary[0].setting_type, TypeId::of::<Home>());
    }

    #[test]
    fn test_itinerary_normalized_one() {
        let mut context = Context::new();
        let person = context.add_person(()).unwrap();
        context
            .register_setting_type(
                Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 5.0,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_type(
                CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 2.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_type(
                School,
                SettingProperties {
                    alpha: 0.2,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 2.5,
                    }),
                },
            )
            .unwrap();

        // Test creating an itinerary with all settings
        let mut itinerary = vec![];
        append_itinerary_entry(&mut itinerary, &context, Home, 1).unwrap();
        append_itinerary_entry(&mut itinerary, &context, CensusTract, 1).unwrap();
        append_itinerary_entry(&mut itinerary, &context, School, 1).unwrap();

        context.add_itinerary(person, itinerary).unwrap();
        let itinerary = context.get_itinerary(person).unwrap();

        let total_ratio: Vec<f64> = itinerary.iter().map(|entry| entry.ratio).collect();
        assert_eq!(total_ratio, vec![0.5, 0.25, 0.25]);
    }
}
