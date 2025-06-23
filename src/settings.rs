use crate::parameters::{
    ContextParametersExt, CoreSettingsTypes, ItinerarySpecificationType, Params,
};
use ixa::{
    define_data_plugin, define_rng, people::Query, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId,
};
use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter, AsRefStr, EnumDiscriminants, IntoStaticStr};
use std::{
    any::{TypeId, Any},
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
    pub itinerary_specification: Option<ItinerarySpecificationType>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash,
    EnumDiscriminants     // create the label-only enum
)]
#[strum_discriminants(    // …and derive the helpers on it
    derive(EnumCount, EnumIter, AsRefStr, IntoStaticStr, Hash),
    name(SettingCategory) // this is the generated enum’s name
)]
pub enum Setting {
    Home(usize),
    CensusTract(usize),
    School(usize),
    Workplace(usize),
    Community(usize),
    HomogeneousMixing(usize),
}

/// `SettingCategory` now exists automatically:
///     SettingCategory::{Home, CensusTract, …}
///     SettingCategory::COUNT
///     SettingCategory::iter()
///     category.as_ref()  (static &str)

impl Setting {
    /// Zero-cost conversion to category
    #[inline]
    pub fn category(self) -> SettingCategory {
        // The macro adds `impl From<Setting> for SettingCategory`.
        self.into()
    }

    #[inline]
    pub const fn id(self) -> usize {
        // This `match` compiles to a single data-load.
        match self {
            Setting::Home(id)
            | Setting::CensusTract(id)
            | Setting::School(id)
            | Setting::Workplace(id) 
            | Setting::Community(id)
            | Setting::HomogeneousMixing(id) => id,
        }
    }

    pub fn calculate_multiplier(&self, member_count: usize, properties: SettingProperties) -> f64 {
        self.category().calculate_multiplier(member_count, properties)
    }
}

impl From<(SettingCategory, usize)> for Setting {
    fn from((cat, id): (SettingCategory, usize)) -> Self {
        match cat {
            SettingCategory::Home        => Setting::Home(id),
            SettingCategory::CensusTract => Setting::CensusTract(id),
            SettingCategory::School      => Setting::School(id),
            SettingCategory::Workplace   => Setting::Workplace(id),
            SettingCategory::Community   => Setting::Community(id),
            SettingCategory::HomogeneousMixing => Setting::HomogeneousMixing(id),
        }
    }
}

impl SettingCategory {
    pub fn calculate_multiplier(&self, member_count: usize, properties: SettingProperties) -> f64 {
        (member_count as f64 - 1.0).powf(properties.alpha)
        // ... or put the function to calculate this inside `SettingProperties` and call it here,
        // or attach it to `Setting` ... etc.
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ItineraryEntry {
    pub setting: Setting,
    pub ratio: f64,
}

impl ItineraryEntry {
    pub fn new(setting_id: Setting, ratio: f64) -> ItineraryEntry {
        ItineraryEntry {
            setting: setting_id,
            ratio,
        }
    }
}

#[allow(dead_code)]
pub enum ItineraryModifiers {
    // Replace itinerary with a new vector of itinerary entries
    ReplaceWith { itinerary: Vec<ItineraryEntry> },
    // Reduce the current itinerary to a setting type (e.g., Home)
    RestrictTo { setting: SettingCategory },
    // Exclude setting types from current itinerary (e.g., Workplace)
    Exclude { setting: SettingCategory },
}

pub fn append_itinerary_entry(
    itinerary: &mut Vec<ItineraryEntry>,
    context: &Context,
    setting: Setting,
) -> Result<(), IxaError> {
    // Is this setting type registered? Our population loader is hard coded to always try to put
    // people in the core setting types, but sometimes we don't want all the core setting types
    // (we didn't specify them). So, first check that the setting in question exists.
    if context
        .get_data_container(SettingDataPlugin)
        .ok_or(IxaError::IxaError(
            "Settings must be initialized prior to making itineraries.".to_string(),
        ))?
        .setting_properties[setting.category() as usize]
        .is_some()
    {
        let ratio = get_itinerary_ratio(context, setting.category())?;
        // No point in adding an itinerary entry if the ratio is zero
        if ratio != 0.0 {
            itinerary.push(ItineraryEntry::new(
                setting,
                ratio
            ));
        }
    }
    Ok(())
}

// In the future, this method could take the person id as an argument for making individual-level
// itineraries.
fn get_itinerary_ratio(
    context: &Context,
    setting_category: SettingCategory,
) -> Result<f64, IxaError> {
    let setting_properties = context
        .get_data_container(SettingDataPlugin)
        .unwrap() // We can unwrap here because we would have already propagated an error in the
        // calling code if the settings data container did not exist.
        .setting_properties[setting_category as usize]
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
    /// None  => not yet registered
    /// Some  => already registered (second registration → error)
    setting_properties: [Option<SettingProperties>; SettingCategory::COUNT],
    // Each inner Vec keyed by `(cat,id)`. If we expect the ID to be just
    // monotonically increasing from 0, we can swap out the HashMap for
    // a Vec.
    members: [HashMap<usize, Vec<PersonId>>; SettingCategory::COUNT],
    itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
    modified_itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
}

impl SettingDataContainer {

    pub fn get_setting_members(
        &self,
        setting: Setting,
    ) -> Option<&Vec<PersonId>> {
        self.members[setting.category() as usize].get(&setting.id())
    }

    pub fn get_setting_members_mut(
        &mut self,
        setting: Setting,
    ) -> &mut Vec<PersonId> {
        self.members[setting.category() as usize]
            .entry(setting.id())
            .or_default()
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
        F: FnMut(Setting, &SettingProperties, &Vec<PersonId>, f64),
    {
        if let Some(itinerary) = self.get_itinerary(person_id) {
            for entry in itinerary {
                let setting = entry.setting;
                // ToDo: If there are no properties for the given setting, either use a default (and
                //       possibly emit a warning), or return an error.
                let setting_props = self.setting_properties[setting.category() as usize].as_ref().unwrap();
                let members = self
                    .get_setting_members(setting)
                    .unwrap();
                callback(setting, setting_props, members, entry.ratio);
            }
        }
    }

    /// Adds the person as a member of each setting in the given itinerary. If the person
    /// already has an itinerary registered, they will first be removed from the settings
    /// of the original itinerary before the itinerary is replaced with the new one.
    fn add_member_to_itinerary_setting(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) {
        // If the person already has an itinerary, remove them from each of the original settings.
        if let Some(old_itinerary) = self.itineraries.get(&person_id).cloned() {
            // ToDo: Emit a warning?
            for entry in &old_itinerary {
                let cat_idx = entry.setting.category() as usize;

                if let Some(vec) = self.members[cat_idx]
                    .get_mut(&entry.setting.id())
                {
                    if let Some(pos) = vec.iter().position(|&pid| pid == person_id) {
                        vec.swap_remove(pos);
                    }
                }
            }
        }
        
        // Now register the new itinerary with the person and add them to each setting.
        for entry in &itinerary {
            self.members[entry.setting.category() as usize]
                .entry(entry.setting.id())
                .or_default()
                .push(person_id);
        }
        
        self.itineraries.insert(person_id, itinerary);
    }

    /// Removes a person from each setting in the given itinerary. If the itinerary is the one
    /// registered for the person, then the person is removed from each setting and the itinerary
    /// is deregistered for that person. If there is an itinerary registered for the person, but
    /// it differs from the given itinerary, then the person is restored to their registered
    /// itinerary.
    pub fn remove_member_from_itinerary_settings(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) {
        let mut actually_removed: HashSet<Setting> = HashSet::new();

        for entry in &itinerary {
            let setting = entry.setting;

            if let Some(vec) =
                self.members[setting.category() as usize].get_mut(&setting.id())
            {
                if let Some(pos) = vec.iter().position(|&pid| pid == person_id)
                {
                    vec.swap_remove(pos);
                    actually_removed.insert(setting);
                }
            }
        }

        let Some(registered) = self.itineraries.get(&person_id) else {
            // nothing to compare
            return;
        };

        // Convert both itineraries to sets of `Setting`
        let reg_set: HashSet<Setting> =
            registered.iter().map(|e| e.setting).collect();
        let req_set: HashSet<Setting> =
            itinerary  .iter().map(|e| e.setting).collect();

        if reg_set == req_set {
            // Same itinerary  ➜  deregister completely
            self.itineraries.remove(&person_id);
            return;
        }

        for entry in registered {
            let setting = entry.setting;
            if actually_removed.contains(&setting) {
                let cat_idx = setting.category() as usize;
                let vec = self.members[cat_idx]
                    .entry(setting.id())
                    .or_default();
                // The person was removed, so restore them.
                vec.push(person_id);
            }
        }
    }
}

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::default()
);

#[allow(dead_code)]
pub trait ContextSettingExt {
    fn get_setting_category_properties(
        &self,
        setting: SettingCategory,
    ) -> Result<SettingProperties, IxaError>;

    fn register_setting_type(
        &mut self,
        setting: SettingCategory,
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
    fn get_setting_of_category_for_person(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory
    ) -> Vec<Setting>;
    fn get_setting_members(
        &self,
        setting: Setting,
    ) -> Option<&Vec<PersonId>>;
    /// Get the total infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of members in the setting
    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64;

    fn get_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>>;

    fn get_contact<Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting: Setting,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError>;
    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError>;
    fn get_setting_for_contact(&self, person_id: PersonId) -> Option<Setting>;
}

trait ContextSettingInternalExt {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting: Setting,
        q: T,
    ) -> Result<Option<PersonId>, IxaError>;

    /// Takes an itinerary and makes it the modified itinerary of `person id`
    /// This modified itinerary is used as the person's itinerary instead of default itinerary
    /// for as long as modified itinerary exists in the container.
    fn add_modified_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError>;

    /// Limit the current itinerary to a specified setting type (e.g., Home)
    /// The proportion of the rest of the settings remains unchanged
    fn limit_itinerary_by_setting_category(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
    ) -> Result<(), IxaError>;

    fn exclude_setting_category_from_itinerary(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
    ) -> Result<(), IxaError>;
}

impl ContextSettingInternalExt for Context {
    fn get_contact_internal<T: Query>(
        &self,
        person_id: PersonId,
        setting: Setting,
        q: T,
    ) -> Result<Option<PersonId>, IxaError> {
        // 1. Pull the membership list or bail out.
        let members = self
            .get_setting_members(setting)
            .ok_or_else(|| IxaError::from("Group membership is None"))?;

        // 2. Caller must belong to the setting.
        if !members.contains(&person_id) {
            return Err(IxaError::from("Attempting contact outside of group membership"));
        }

        // 3. If they’re alone, there can be no contact.
        if members.len() == 1 {
            return Ok(None);
        }

        // 4. Build the pool of eligible contacts.
        let candidates: Vec<PersonId> = if q.get_query().is_empty() {
            members
                .iter()
                .filter(|&&pid| pid != person_id)
                .copied()
                .collect()
        } else {
            members
                .iter()
                .filter(|&&pid| pid != person_id && self.match_person(pid, q))
                .copied()
                .collect()
        };

        // 5. Nothing matched the query?  Return None.
        if candidates.is_empty() {
            return Ok(None);
        }

        // 6. Uniformly sample one candidate.
        let idx = self.sample_range(SettingsRng, 0..candidates.len());
        Ok(Some(candidates[idx]))
    }


    fn add_modified_itinerary(
        &mut self,
        person_id: PersonId,
        mut itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        // Normalize itinerary ratios
        self.validate_itinerary(&itinerary)?;

        let sum: f64 = itinerary.iter().map(|e| e.ratio).sum();
        for e in &mut itinerary {
            // safe: sum > 0 after validation
            e.ratio /= sum;
        }

        let container = self.get_data_container_mut(SettingDataPlugin);

        // ToDo: If a modified itinerary exists, should we replace it with this? See method doc comments
        if container.modified_itineraries.contains_key(&person_id) {
            return Err(IxaError::from(
                 "Can't modify itinerary because a modified itinerary is already present. Remove and add new modified itinerary."
             ));
        }

        if !container.itineraries.contains_key(&person_id) {
            return Err(IxaError::from(
                "Can't modify itinerary if there isn't one present",
            ));
        }

        container.add_member_to_itinerary_setting(person_id, itinerary.clone());
        container.modified_itineraries.insert(person_id, itinerary);

        Ok(())
    }

    fn limit_itinerary_by_setting_category(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
    ) -> Result<(), IxaError> {
        // 1. fetch the person’s current itinerary or bail out early
        let container = self.get_data_container_mut(SettingDataPlugin);
        let itinerary = container
            .itineraries
            .get(&person_id)
            .ok_or_else(|| IxaError::from("Can't find itinerary for person"))?;

        // 2. keep only entries that match the requested category
        let modified_itinerary: Vec<ItineraryEntry> = itinerary
            .iter()
            .copied()
            .filter(|e| e.setting.category() == setting_category)
            .collect();

        if modified_itinerary.is_empty() {
            return Err(IxaError::from(
                "limit itinerary resulted in empty modified itinerary",
            ));
        }

        // 3. hand off to the existing helper (normalises & validates)
        self.add_modified_itinerary(person_id, modified_itinerary)
    }

    fn exclude_setting_category_from_itinerary(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
    ) -> Result<(), IxaError> {
        // 1. fetch the person’s current itinerary or bail out
        let container = self.get_data_container_mut(SettingDataPlugin);
        let itinerary = container
            .itineraries
            .get(&person_id)
            .ok_or_else(|| IxaError::from("Can't find itinerary for person"))?;

        // 2. drop every entry that matches the category we’re excluding
        let modified_itinerary: Vec<ItineraryEntry> = itinerary
            .iter()
            .copied()
            .filter(|e| e.setting.category() != setting_category)
            .collect();

        if modified_itinerary.is_empty() {
            return Err(IxaError::from(
                "Exclude itinerary resulted in empty modified itinerary",
            ));
        }

        // 3. hand off to the existing helper (normalises & validates)
        self.add_modified_itinerary(person_id, modified_itinerary)
    }
}

impl ContextSettingExt for Context {
    fn get_setting_category_properties(
        &self,
        cat: SettingCategory,
    ) -> Result<SettingProperties, IxaError> {
        self.get_data_container(SettingDataPlugin)
            .ok_or_else(|| IxaError::IxaError("Setting plugin data is none".into()))?
            .setting_properties[cat as usize]            // fixed-size array slot
            .ok_or_else(|| IxaError::from(
                "Attempting to get properties of unregistered setting type",
            ))
    }

    fn register_setting_type(
        &mut self,
        setting_category: SettingCategory,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError> {

        Ok(())
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

        container.add_member_to_itinerary_setting(person_id, itinerary.clone());
        container.itineraries.insert(person_id, itinerary);

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
                    setting.type_id()
                );
                self.limit_itinerary_by_setting_category(person_id, setting)
            }
            ItineraryModifiers::Exclude { setting } => {
                trace!(
                    "ItineraryModifier::Exclude person {person_id}-- {:?}",
                    setting.type_id()
                );
                self.exclude_setting_category_from_itinerary(person_id, setting)
            }
        }
    }

    fn remove_modified_itinerary(&mut self, person_id: PersonId) -> Result<(), IxaError> {
        let container = self.get_data_container_mut(SettingDataPlugin);

        // If there's a modified itinerary present, remove
        if let Some(previous_mod_itinerary) = container.modified_itineraries.get(&person_id) {
            container
                .remove_member_from_itinerary_settings(person_id, previous_mod_itinerary.clone());
        }

        container.modified_itineraries.remove(&person_id);

        Ok(())
    }

    fn validate_itinerary(&self, itinerary: &[ItineraryEntry]) -> Result<(), IxaError> {
        use std::collections::HashSet;

        let mut seen = HashSet::with_capacity(itinerary.len());

        for entry in itinerary {
            // ratio must be non-negative
            if entry.ratio < 0.0 {
                return Err(IxaError::from(
                    "Setting ratio must be greater than or equal to 0",
                ));
            }

            // ToDo: This is incorrect. A single `Setting` may only appear once, but you can have
            //       two workplaces, for example.
            // each category may appear at most once
            let cat = entry.setting.category();
            if !seen.insert(cat) {
                return Err(IxaError::from("Duplicated setting"));
            }
        }

        Ok(())
    }

    fn get_setting_of_category_for_person(
        &mut self,
        person_id: PersonId,
        category: SettingCategory,
    ) -> Vec<Setting> {
        let container = self.get_data_container_mut(SettingDataPlugin);
        container
            .itineraries
            .get(&person_id)
            .map_or(Vec::new(), |its| {
                its.iter()
                   .filter(|e| e.setting.category() == category)
                   .map(|e| e.setting)
                   .collect()
            })
    }

    // Erik to do: is this method redundant?
    fn get_setting_members(
        &self,
        setting: Setting,
    ) -> Option<&Vec<PersonId>> {
        self.get_data_container(SettingDataPlugin)?
            .get_setting_members(setting)
    }

    fn calculate_total_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut collector = 0.0;
        container.with_itinerary(person_id, |setting, setting_props, members, ratio| {
            let multiplier = setting.calculate_multiplier(members.len(), *setting_props);
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

    fn get_contact<Q: Query + 'static>(
        &self,
        person_id: PersonId,
        setting: Setting,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError> {
        // let container: &SettingDataContainer = self.get_data_container(SettingDataPlugin).unwrap();
        self.get_contact_internal(
            person_id,
            setting,
            q,
        )
    }

    fn draw_contact_from_transmitter_itinerary<Q: Query>(
        &self,
        person_id: PersonId,
        q: Q,
    ) -> Result<Option<PersonId>, IxaError> {
        // 1.  If the person has no itinerary, bail out early.
        let itinerary = match self.get_itinerary(person_id) {
            Some(it) => it,
            None     => return Ok(None),
        };

        // 2.  Build the weight vector in one iterator pass.
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let weights: Vec<f64> = itinerary
            .iter()
            .map(|entry| {
                let members   = container.get_setting_members(entry.setting).unwrap();
                let props     = container
                    .setting_properties[entry.setting.category() as usize]
                    .unwrap();
                entry.ratio * entry.setting.calculate_multiplier(members.len(), props)
            })
            .collect();

        // 3.  Sample a setting, then delegate to the internal contact picker.
        let idx = self.sample_weighted(SettingsRng, &weights);
        self.get_contact_internal(person_id, itinerary[idx].setting, q)
    }

    fn get_setting_for_contact(&self, person_id: PersonId) -> Option<Setting> {
        let container = self.get_data_container(SettingDataPlugin).unwrap();
        let mut itinerary_multiplier = Vec::new();
        container.with_itinerary(person_id, |setting_type, setting_props, members, ratio| {
            let multiplier = setting_type.calculate_multiplier(members.len(), *setting_props);
            itinerary_multiplier.push(ratio * multiplier);
        });

        let setting_index = self.sample_weighted(SettingsRng, &itinerary_multiplier);

        if let Some(itinerary) = self.get_itinerary(person_id) {
            let itinerary_entry = &itinerary[setting_index];
            Some(itinerary_entry.setting)
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
                    .register_setting_type(SettingCategory::Home, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::CensusTract => {
                context
                    .register_setting_type(SettingCategory::CensusTract, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::School => {
                context
                    .register_setting_type(SettingCategory::School, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::Workplace => {
                context
                    .register_setting_type(SettingCategory::Workplace, setting_properties)
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
    use SettingCategory::*;

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
        let home_props = context.get_setting_category_properties(Home).unwrap();
        let tract_props = context.get_setting_category_properties(CensusTract).unwrap();

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
        let e = context.get_setting_category_properties(Home).err();
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
        context.get_setting_category_properties(Home).unwrap();
        let e = context.get_setting_category_properties(CensusTract).err();
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
            ItineraryEntry::new(Setting::Home(2), 0.5),
            ItineraryEntry::new(Setting::Home(2), 0.5),
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
        let itinerary = vec![ItineraryEntry::new(Setting::Home(1), -0.5)];

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
        let itinerary = vec![ItineraryEntry::new(Setting::Community(1), 0.5)];

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
            ItineraryEntry::new(Setting::Home(1), 0.5),
            ItineraryEntry::new(Setting::Home(2), 0.5),
        ];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(Setting::Home(2))
            .unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_person(()).unwrap();
        let itinerary2 = vec![ItineraryEntry::new(Setting::Home(2), 1.0)];
        context.add_itinerary(person2, itinerary2).unwrap();

        let members2 = context
            .get_setting_members(Setting::Home(2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let members2 = context
            .get_setting_members(Setting::Home(2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let itinerary3 = vec![ItineraryEntry::new(Setting::Home(3), 0.5)];
        context.add_itinerary(person, itinerary3).unwrap();
        let members2_removed = context
            .get_setting_members(Setting::Home(2))
            .unwrap();
        assert_eq!(members2_removed.len(), 1);
        let members3 = context
            .get_setting_members(Setting::Home(3))
            .unwrap();
        assert_eq!(members3.len(), 1);
        let members1_removed = context
            .get_setting_members(Setting::Home(1))
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
            ItineraryEntry::new(Setting::Home(0), 0.5),
            ItineraryEntry::new(Setting::Workplace(0), 0.5),
        ];

        let itinerary_two = vec![ItineraryEntry::new(Setting::Workplace(0), 1.0)];

        context.add_itinerary(person, itinerary).unwrap();
        context.add_itinerary(person_two, itinerary_two).unwrap();

        let h_id = context.get_setting_of_category_for_person(person, Home);
        let w_id = context.get_setting_of_category_for_person(person_two, Workplace);

        let h_members = context
            .get_setting_members(h_id[0])
            .unwrap();
        let w_members = context
            .get_setting_members(w_id[0])
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
            ItineraryEntry::new(Setting::Home(0), 1.0),
            ItineraryEntry::new(Setting::Workplace(0), 1.0),
        ];
        let isolation_itinerary = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];

        let _ = context.add_itinerary(person, itinerary);

        let members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
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
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 0);
    }

    #[test]
    fn test_itinerary_modifiers_replace() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let itinerary_vec: Vec<Vec<(SettingCategory, usize)>> = vec![
            vec![(Home, 0), (Workplace, 0), (School, 0)],
            vec![(Home, 0), (Workplace, 0)],
            vec![(Home, 0), (Workplace, 0)],
            vec![(Home, 1), (School, 0)],
            vec![(Home, 1), (Workplace, 0)],
            vec![(Home, 1), (Workplace, 0)],
        ];

        let mut person_0: Option<PersonId> = None;
        for (p_id, p_it) in itinerary_vec.iter().enumerate() {
            let mut p_itinerary = Vec::<ItineraryEntry>::new();
            for (s_type, s_id) in p_it.clone() {
                p_itinerary.push(ItineraryEntry::new((s_type, s_id).into(), 1.0));
            }
            let person = context.add_person(()).unwrap();
            let _e = context.add_itinerary(person, p_itinerary);
            if p_id == 0 {
                person_0 = Some(person);
            }
        }
        let alpha_h = context.get_setting_category_properties(Home).unwrap().alpha;
        let alpha_w = context.get_setting_category_properties(Workplace).unwrap().alpha;
        let alpha_s = context.get_setting_category_properties(School).unwrap().alpha;

        let inf_multiplier =
            context.calculate_total_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = (1.0 / 3.0) * (2_f64).powf(alpha_h)
            + (1.0 / 3.0) * (4_f64).powf(alpha_w)
            + (1.0 / 3.0) * (1_f64).powf(alpha_s);

        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.0);

        // 2. Isolate person with itinerary [(Home 0 , 0.95), (Workplace 0, 0.05)]
        let isolation_itinerary = vec![
            ItineraryEntry::new(Setting::Home(0), 0.95),
            ItineraryEntry::new(Setting::Workplace(0), 0.05),
        ];

        let _ = context.modify_itinerary(
            person_0.unwrap(),
            ItineraryModifiers::ReplaceWith {
                itinerary: isolation_itinerary,
            },
        );

        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let h_one_members = context
            .get_setting_members(Setting::Home(1))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        let s_members = context
            .get_setting_members(Setting::School(0))
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
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let h_one_members = context
            .get_setting_members(Setting::Home(1))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        let s_members = context
            .get_setting_members(Setting::School(0))
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
            ItineraryEntry::new(Setting::Home(0), 1.0),
            ItineraryEntry::new(Setting::Workplace(0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(Setting::Workplace(0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (limit default): {h_members:?}");
        println!("WORK MEMBERS (limit default): {w_members:?}");

        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(person, ItineraryModifiers::RestrictTo { setting: Home });

        // Check membership
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (limit isolation): {h_members:?}");
        println!("WORK MEMBERS (limit isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary(person);
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
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
            ItineraryEntry::new(Setting::Home(0), 1.0),
            ItineraryEntry::new(Setting::Workplace(0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(Setting::Workplace(0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (exclude default): {h_members:?}");
        println!("WORK MEMBERS (exclude default): {w_members:?}");

        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::Exclude {
                setting: Workplace,
            },
        );

        // Check membership
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (exclude isolation): {h_members:?}");
        println!("WORK MEMBERS (exclude isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary(person);
        let h_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let w_members = context
            .get_setting_members(Setting::Workplace(0))
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
                    ItineraryEntry::new(Setting::Home(s), 0.5),
                    ItineraryEntry::new(Setting::CensusTract(s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let members = context
                .get_setting_members(Setting::Home(s))
                .unwrap();
            let tract_members = context
                .get_setting_members(Setting::CensusTract(s))
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
                let itinerary = vec![ItineraryEntry::new(Setting::Home(s), 0.5)];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(Setting::Home(home_id), 0.5)];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(Setting::Home(home_id))
            .unwrap();

        let setting_type = Home;

        let inf_multiplier = setting_type.calculate_multiplier(
            members.len(),
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
                    ItineraryEntry::new(Setting::Home(s), 0.5),
                    ItineraryEntry::new(Setting::CensusTract(s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];
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
            ItineraryEntry::new(Setting::Home(0), 0.5),
            ItineraryEntry::new(Setting::CensusTract(0), 0.5),
        ];
        context.add_itinerary(person, itinerary_complete).unwrap();
        let members_home = context
            .get_setting_members(Setting::Home(0))
            .unwrap();
        let members_tract = context
            .get_setting_members(Setting::CensusTract(0))
            .unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings =
            context.calculate_total_infectiousness_multiplier_for_person(person);

        let alpha_h = context.get_setting_category_properties(Home).unwrap().alpha;
        let alpha_ct = context.get_setting_category_properties(CensusTract).unwrap().alpha;

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
            ItineraryEntry::new(Setting::Home(0), 0.5),
            ItineraryEntry::new(Setting::CensusTract(0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();

        // When person a is used to select a setting for contact, it should return Home. While they are
        // also a member of CensusTract, since they are the only member the multiplier used to weight the
        // selection is 0.0 from calculate_multiplier. Thus the probability CensusTract is selected is 0.0.
        let setting_id = context.get_setting_for_contact(person_a).unwrap();
        assert_eq!(setting_id.category(), Home);
        assert_eq!(setting_id.id(), 0);

        let setting_id = context.get_setting_for_contact(person_b).unwrap();
        assert_eq!(setting_id.category(), Home);
        assert_eq!(setting_id.id(), 0);

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(Setting::CensusTract(0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();
        let setting_id = context.get_setting_for_contact(person_c).unwrap();
        assert_eq!(
            setting_id.category(),
            CensusTract
        );
        assert_eq!(setting_id.id(), 0);
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
            ItineraryEntry::new(Setting::Home(0), 0.5),
            ItineraryEntry::new(Setting::CensusTract(0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();
        let setting_id = context.get_setting_for_contact(person_a).unwrap();
        assert_eq!(
            Some(person_b),
            context.get_contact(person_a, setting_id, ()).unwrap()
        );

        assert!(context
            .get_contact(person_a, Setting::CensusTract(0), ())
            .unwrap()
            .is_none());

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(Setting::CensusTract(0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();

        let e = context
            .get_contact(person_b, Setting::CensusTract(0), ())
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

        let e = context.get_contact(person_b, Setting::CensusTract(10), ());
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
                let itinerary = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];
                context.add_itinerary(person, itinerary).unwrap();
            }

            for _ in 0..3 {
                let person = context.add_person(()).unwrap();
                let itinerary = vec![ItineraryEntry::new(Setting::CensusTract(0), 1.0)];
                context.add_itinerary(person, itinerary).unwrap();
            }

            let person = context.add_person(()).unwrap();
            let itinerary_home = vec![
                ItineraryEntry::new(Setting::Home(0), 1.0),
                ItineraryEntry::new(Setting::CensusTract(0), 0.0),
            ];
            let itinerary_censustract = vec![
                ItineraryEntry::new(Setting::Home(0), 0.0),
                ItineraryEntry::new(Setting::CensusTract(0), 1.0),
            ];
            let home_members = context
                .get_setting_members(Setting::Home(0))
                .unwrap()
                .clone();
            let tract_members = context
                .get_setting_members(Setting::CensusTract(0))
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
            let itinerary = vec![ItineraryEntry::new(Setting::Home(0), 1.0)];
            context.add_itinerary(person, itinerary).unwrap();
        }

        for i in 3..6 {
            let person = context.add_person((Age, 39 + i)).unwrap();
            let itinerary = vec![ItineraryEntry::new(Setting::CensusTract(0), 1.0)];
            context.add_itinerary(person, itinerary).unwrap();
        }

        let person = context.add_person((Age, 42)).unwrap();
        let itinerary_home = vec![
            ItineraryEntry::new(Setting::Home(0), 1.0),
            ItineraryEntry::new(Setting::CensusTract(0), 0.0),
        ];
        let itinerary_censustract = vec![
            ItineraryEntry::new(Setting::Home(0), 0.0),
            ItineraryEntry::new(Setting::CensusTract(0), 1.0),
        ];
        let home_members = context
            .get_setting_members(Setting::Home(0))
            .unwrap()
            .clone();
        let tract_members = context
            .get_setting_members(Setting::CensusTract(0))
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
        append_itinerary_entry(&mut itinerary, &context, Setting::Home(1)).unwrap();
        assert_eq!(itinerary.len(), 1);
        assert_eq!(itinerary[0].setting.category(), Home);
        assert_eq!(itinerary[0].setting.id(), 1);
        assert_almost_eq!(itinerary[0].ratio, 0.5, 0.0);

        // Test appending an entry with a different setting type
        append_itinerary_entry(&mut itinerary, &context, Setting::School(42)).unwrap();
        assert_eq!(itinerary.len(), 2);
        assert_eq!(itinerary[1].setting.category(), School);
        assert_eq!(itinerary[1].setting.id(), 42);
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
        append_itinerary_entry(&mut iitinerary, &context, Setting::Workplace(1)).unwrap();

        assert_eq!(iitinerary.len(), 0);

        append_itinerary_entry(&mut iitinerary, &context, Setting::Home(1)).unwrap();
        assert_eq!(iitinerary.len(), 1);
        assert_eq!(iitinerary[0].setting.category(), Home);
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
        append_itinerary_entry(&mut itinerary, &context, Setting::Home(1)).unwrap();
        append_itinerary_entry(&mut itinerary, &context, Setting::CensusTract(1)).unwrap();
        append_itinerary_entry(&mut itinerary, &context, Setting::School(1)).unwrap();

        context.add_itinerary(person, itinerary).unwrap();
        let itinerary = context.get_itinerary(person).unwrap();

        let total_ratio: Vec<f64> = itinerary.iter().map(|entry| entry.ratio).collect();
        assert_eq!(total_ratio, vec![0.5, 0.25, 0.25]);
    }
}
