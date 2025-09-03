use crate::{
    interventions::ContextItineraryModifierExt,
    parameters::{ContextParametersExt, CoreSettingsTypes, ItinerarySpecificationType, Params},
};
use ixa::{
    define_data_plugin, define_rng, trace, Context, ContextPeopleExt, ContextRandomExt, IxaError,
    PersonId, PluginContext,
};
use serde::{Deserialize, Serialize};

use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    hash::Hash,
};

use dyn_clone::DynClone;

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
    pub itinerary_specification: Option<ItinerarySpecificationType>,
}

pub trait SettingCategory: std::fmt::Debug + 'static {
    fn get_type_id(&self) -> std::any::TypeId;
}

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub struct SettingId<T: SettingCategory> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    _phantom: std::marker::PhantomData<T>,
}

pub trait AnySettingId
where
    Self: std::fmt::Debug + DynClone + 'static,
{
    fn id(&self) -> usize;
    fn calculate_multiplier(
        &self,
        members: &HashSet<PersonId>,
        setting_properties: SettingProperties,
    ) -> f64;
    fn get_category_id(&self) -> &'static str;
    fn get_type_id(&self) -> TypeId;
    fn get_tuple_id(&self) -> (TypeId, usize) {
        (self.get_type_id(), self.id())
    }
}

dyn_clone::clone_trait_object!(AnySettingId);

impl<T: SettingCategory + Clone> AnySettingId for SettingId<T> {
    fn id(&self) -> usize {
        self.id
    }
    fn get_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    #[allow(clippy::cast_precision_loss)]
    fn calculate_multiplier(
        &self,
        members: &HashSet<PersonId>,
        setting_properties: SettingProperties,
    ) -> f64 {
        ((members.len() - 1) as f64).powf(setting_properties.alpha)
    }
    fn get_category_id(&self) -> &'static str {
        std::any::type_name::<T>()
            .rsplit("::")
            .next()
            .unwrap_or_default()
    }
}

impl<T: SettingCategory> SettingId<T> {
    pub fn new(_category: T, id: usize) -> SettingId<T> {
        SettingId {
            id,
            _phantom: std::marker::PhantomData::<T>,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItineraryEntry {
    pub setting: Box<dyn AnySettingId>,
    pub ratio: f64,
}

impl ItineraryEntry {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(setting: impl AnySettingId, ratio: f64) -> ItineraryEntry {
        let boxed_setting = Box::new(setting);
        ItineraryEntry {
            setting: boxed_setting,
            ratio,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum ItineraryModifiers<'a> {
    // Replace itinerary with a new vector of itinerary entries
    ReplaceWith {
        itinerary: Vec<ItineraryEntry>,
        ranking: usize,
    },
    // Reduce the current itinerary to a setting type (e.g., Home)
    RestrictTo {
        setting: &'a dyn SettingCategory,
        ranking: usize,
    },
    // Exclude setting types from current itinerary (e.g., Workplace)
    Exclude {
        setting: &'a dyn SettingCategory,
        ranking: usize,
    },
}

pub fn append_itinerary_entry(
    itinerary: &mut Vec<ItineraryEntry>,
    context: &Context,
    setting: impl AnySettingId,
    nondefault_ratio: Option<f64>,
) -> Result<(), IxaError> {
    // Is this setting type registered? Our population loader is hard coded to always try to put
    // people in the core setting types, but sometimes we don't want all the core setting types
    // (we didn't specify them). So, first check that the setting in question exists.
    if context
        .get_data(SettingDataPlugin)
        .setting_properties
        .contains_key(&setting.get_type_id())
    {
        let ratio = match nondefault_ratio {
            Some(user_input) => user_input,
            None => get_itinerary_ratio(context, &setting)?,
        };
        itinerary.push(ItineraryEntry::new(setting, ratio));
    }
    Ok(())
}

// In the future, this method could take the person id as an argument for making individual-level
// itineraries.
fn get_itinerary_ratio(context: &Context, setting: &dyn AnySettingId) -> Result<f64, IxaError> {
    let setting_properties = context
        .get_data(SettingDataPlugin)
        .setting_properties
        .get(&setting.get_type_id())
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
    setting_categories: HashSet<TypeId>,
    // For each setting type (e.g., Home) store the properties (e.g., alpha)
    setting_properties: HashMap<TypeId, SettingProperties>,
    // For each setting type, have a map of each setting id and a list of members
    active_members: HashMap<(TypeId, usize), HashMap<PersonId, usize>>,
    active_members_set: HashMap<(TypeId, usize), HashSet<PersonId>>,
    active_members_vec: HashMap<(TypeId, usize), Vec<PersonId>>,
    inactive_members: HashMap<(TypeId, usize), HashSet<PersonId>>,
    all_members: HashMap<(TypeId, usize), HashSet<PersonId>>,
    itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
    modified_itineraries: HashMap<PersonId, HashMap<usize, Vec<ItineraryEntry>>>,
}

#[derive(Clone, Copy)]
enum ItinerarySelector {
    Default,
    Current,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
// Inactive membership selectors are not called
enum MembershipSelector {
    Active,
    Inactive,
    Union,
}

impl SettingDataContainer {
    fn get_setting_members(
        &self,
        setting: &dyn AnySettingId,
        selector: MembershipSelector,
    ) -> Option<&HashSet<PersonId>> {
        match selector {
            MembershipSelector::Active => self.get_active_setting_members(setting),
            MembershipSelector::Inactive => self.get_inactive_setting_members(setting),
            MembershipSelector::Union => self.get_all_setting_members(setting),
        }
    }
    fn get_active_setting_members(&self, setting: &dyn AnySettingId) -> Option<&HashSet<PersonId>> {
        self.active_members_set.get(&setting.get_tuple_id())
    }
    fn get_active_setting_members_vec(&self, setting: &dyn AnySettingId) -> Option<&Vec<PersonId>> {
        self.active_members_vec.get(&setting.get_tuple_id())
    }
    fn get_inactive_setting_members(
        &self,
        setting: &dyn AnySettingId,
    ) -> Option<&HashSet<PersonId>> {
        self.inactive_members.get(&setting.get_tuple_id())
    }
    fn get_all_setting_members(&self, setting: &dyn AnySettingId) -> Option<&HashSet<PersonId>> {
        self.all_members.get(&setting.get_tuple_id())
    }
    fn get_default_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        self.itineraries.get(&person_id)
    }
    fn get_dominate_modified_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        self.modified_itineraries.get(&person_id).and_then(
            |v: &HashMap<usize, Vec<ItineraryEntry>>| {
                v.iter().min_by_key(|(k, _)| *k).map(|(_, entries)| entries)
            },
        )
    }
    fn get_dominate_modified_itinerary_ranking(&self, person_id: PersonId) -> Option<usize> {
        self.modified_itineraries.get(&person_id).and_then(
            |v: &HashMap<usize, Vec<ItineraryEntry>>| {
                v.iter().min_by_key(|(k, _)| *k).map(|(k, _)| *k)
            },
        )
    }
    fn get_itinerary(
        &self,
        person_id: PersonId,
        selector: ItinerarySelector,
    ) -> Option<&Vec<ItineraryEntry>> {
        // Assume modified is current if specified
        match selector {
            ItinerarySelector::Default => self.get_default_itinerary(person_id),
            ItinerarySelector::Current => {
                let modified = self.get_dominate_modified_itinerary(person_id);
                if modified.is_some() {
                    modified
                } else {
                    self.get_default_itinerary(person_id)
                }
            }
        }
    }
    fn with_itinerary<F>(
        &self,
        person_id: PersonId,
        itinerary_selector: ItinerarySelector,
        membership_selector: MembershipSelector,
        mut callback: F,
    ) where
        F: FnMut(&dyn AnySettingId, &SettingProperties, &HashSet<PersonId>, f64),
    {
        if let Some(itinerary) = self.get_itinerary(person_id, itinerary_selector) {
            for entry in itinerary {
                let setting = entry.setting.as_ref();
                let setting_props = self
                    .setting_properties
                    .get(&entry.setting.get_type_id())
                    .unwrap();
                let members = self
                    .get_setting_members(setting, membership_selector)
                    .unwrap();
                callback(setting, setting_props, members, entry.ratio);
            }
        }
    }
    fn activate_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: &Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let _span = open_span("activate itinerary");
        for itinerary_entry in itinerary {
            // TODO: If we are changing a person's itinerary, the person_id should be removed from vector
            // This isn't the same as the concept of being present or not.
            if !self
                .setting_categories
                .contains(&itinerary_entry.setting.get_type_id())
            {
                return Err(IxaError::from(
                    "Itinerary entry setting type not registered",
                ));
            }
            self.set_member_activity(
                person_id,
                itinerary_entry.ratio,
                itinerary_entry.setting.get_tuple_id(),
            );
        }
        Ok(())
    }
    fn set_member_activity(
        &mut self,
        person_id: PersonId,
        ratio: f64,
        setting_identifier: (TypeId, usize),
    ) {
        if ratio > 0.0 {
            self.add_active_member(person_id, setting_identifier);
        } else {
            self.add_inactive_member(person_id, setting_identifier);
        }
    }

    fn add_active_member(&mut self, person_id: PersonId, setting_identifier: (TypeId, usize)) {
        let map = self.active_members.entry(setting_identifier).or_default();
        if map.get(&person_id).is_none() {
            map.insert(person_id, map.len());
            self.active_members_set
                .entry(setting_identifier)
                .or_default()
                .insert(person_id);
            self.active_members_vec
                .entry(setting_identifier)
                .or_default()
                .push(person_id);
        }
        self.inactive_members
            .entry(setting_identifier)
            .or_default()
            .remove(&person_id);
        self.all_members
            .entry(setting_identifier)
            .or_default()
            .insert(person_id);
    }

    fn add_inactive_member(&mut self, person_id: PersonId, setting_identifier: (TypeId, usize)) {
        let map = self.active_members.entry(setting_identifier).or_default();
        if let Some(&index) = map.get(&person_id) {
            let v = self
                .active_members_vec
                .get_mut(&setting_identifier)
                .unwrap();

            if v.len() > 1 {
                let old_member = v[v.len() - 1];
                map.insert(old_member, index);
            }

            v.swap_remove(index);
            map.remove(&person_id);

            self.active_members_set
                .entry(setting_identifier)
                .or_default()
                .remove(&person_id);
        }
        self.inactive_members
            .entry(setting_identifier)
            .or_default()
            .insert(person_id);
        self.all_members
            .entry(setting_identifier)
            .or_default()
            .insert(person_id);
    }

    fn deactivate_itinerary(&mut self, person_id: PersonId, itinerary: Vec<ItineraryEntry>) {
        let _span = open_span("deactivate itinerary");
        for itinerary_entry in itinerary {
            self.add_inactive_member(person_id, itinerary_entry.setting.get_tuple_id());
        }
    }
}

#[macro_export]
macro_rules! define_setting_category {
    ($name:ident) => {
        #[derive(Default, Copy, Clone, Debug, Hash, Eq, PartialEq)]
        pub struct $name;

        impl $crate::settings::SettingCategory for $name {
            fn get_type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<$name>()
            }
        }
    };
}

use crate::profiling::open_span;
pub use define_setting_category;

define_setting_category!(Home);
define_setting_category!(CensusTract);
define_setting_category!(School);
define_setting_category!(Workplace);

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::default()
);

trait ContextSettingInternalExt:
    PluginContext + ContextRandomExt + ContextItineraryModifierExt
{
    /// Takes an itinerary and adds makes it the modified itinerary of `person id`
    /// This modified itinerary is used as the person's itinerary instead of default itinerary
    /// for as long as modified itinerary exists in the container.
    fn add_modified_itinerary(
        &mut self,
        person_id: PersonId,
        mut itinerary: Vec<ItineraryEntry>,
        settings_change: bool,
        ranking: usize,
    ) -> Result<(), IxaError> {
        // Normalize itinerary ratios
        self.validate_itinerary(&itinerary)?;

        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        // If we passed validation, we know setting entries aren't all zero, so we can divide by
        // total_ratio without worrying about dividing by zero.
        for entry in &mut itinerary {
            entry.ratio /= total_ratio;
        }
        let container = self.get_data_mut(SettingDataPlugin);

        // Check if a modified itinerary with the same ranking already exists
        if let Some(modified_itineraries) = container.modified_itineraries.get(&person_id) {
            if modified_itineraries.contains_key(&ranking) {
                return Err(IxaError::from(
                    format!("Can't add modified itinerary: ranking {ranking} already exists for person {person_id}. Remove it before adding a new one.")
                ));
            }
        }

        // Remove people from default itinerary, if there's one
        match container.itineraries.get(&person_id) {
            None => {
                return Err(IxaError::from(
                    "Can't modify itinerary if there isn't one present",
                ))
            }
            Some(previous_itinerary) => {
                if settings_change {
                    container.deactivate_itinerary(person_id, previous_itinerary.clone());
                }
            }
        }

        container
            .modified_itineraries
            .entry(person_id)
            .or_default()
            .insert(ranking, itinerary.clone());
        let dominant_itinerary = container
            .get_dominate_modified_itinerary(person_id)
            .ok_or_else(|| IxaError::from("Can't find dominate modified itinerary"))?
            .clone();
        container.activate_itinerary(person_id, &dominant_itinerary)?;

        Ok(())
    }

    fn exclude_setting_from_itinerary(
        &mut self,
        person_id: PersonId,
        setting: &dyn SettingCategory,
        ranking: usize,
    ) -> Result<(), IxaError> {
        let container = self.get_data_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Err(IxaError::from("Can't find itinerary for person")),
            Some(itinerary_vector) => {
                let mut modified_itinerary = Vec::<ItineraryEntry>::new();
                for entry in itinerary_vector {
                    let mut new_entry = entry.clone();
                    if entry.setting.get_type_id() == setting.get_type_id() {
                        new_entry.ratio = 0.0;
                    }
                    modified_itinerary.push(new_entry);
                }
                if modified_itinerary.is_empty() {
                    return Err(IxaError::from(
                        "Exclude itinerary resulted in empty modified itinerary",
                    ));
                }

                self.add_modified_itinerary(person_id, modified_itinerary, false, ranking)?;
                Ok(())
            }
        }
    }
    /// Limit the current itinerary to a specified setting type (e.g., Home)
    /// The proportion of the rest of the settings remains unchanged
    fn limit_itinerary_by_setting_category(
        &mut self,
        person_id: PersonId,
        setting: &dyn SettingCategory,
        ranking: usize,
    ) -> Result<(), IxaError> {
        let container = self.get_data_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Err(IxaError::from("Can't find itinerary for person")),
            Some(itineraries) => {
                let mut modified_itinerary = Vec::<ItineraryEntry>::new();
                for entry in itineraries {
                    let mut new_entry = entry.clone();
                    if entry.setting.get_type_id() != setting.get_type_id() {
                        new_entry.ratio = 0.0;
                    }
                    modified_itinerary.push(new_entry);
                }
                if modified_itinerary.is_empty() {
                    return Err(IxaError::from(
                        "limit itinerary resulted in empty modified itinerary",
                    ));
                }

                self.add_modified_itinerary(person_id, modified_itinerary, false, ranking)?;
                Ok(())
            }
        }
    }

    fn validate_itinerary(&self, itinerary: &[ItineraryEntry]) -> Result<(), IxaError> {
        let mut setting_counts: HashMap<TypeId, HashSet<usize>> = HashMap::new();
        let _span = open_span("validate_modified_itinerary");
        for itinerary_entry in itinerary {
            let setting_id = itinerary_entry.setting.id();
            let setting_type = itinerary_entry.setting.get_type_id();
            if let Some(setting_count_set) = setting_counts.get(&setting_type) {
                if setting_count_set.contains(&setting_id) {
                    return Err(IxaError::from("Duplicated setting".to_string()));
                }
            }
            setting_counts
                .entry(setting_type)
                .or_default()
                .insert(setting_id);

            if itinerary_entry.ratio < 0.0 {
                return Err(IxaError::from(
                    "Setting ratio must be greater than or equal to 0".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn get_setting_members_internal(
        &self,
        setting: &dyn AnySettingId,
        selector: MembershipSelector,
    ) -> Option<&HashSet<PersonId>> {
        self.get_data(SettingDataPlugin)
            .get_setting_members(setting, selector)
    }

    fn sample_active_setting_members(&self, setting: &dyn AnySettingId) -> Option<PersonId> {
        if let Some(members) = self
            .get_data(SettingDataPlugin)
            .get_active_setting_members_vec(setting)
        {
            if members.is_empty() {
                return None;
            }
            let person = members[self.sample_range(SettingsRng, 0..members.len())];
            return Some(person);
        }
        None
    }

    fn get_itinerary(
        &self,
        person_id: PersonId,
        selector: ItinerarySelector,
    ) -> Option<&Vec<ItineraryEntry>> {
        self.get_data(SettingDataPlugin)
            .get_itinerary(person_id, selector)
    }
}
impl ContextSettingInternalExt for Context {}

fn identical_settings(
    itinerary_0: &Vec<ItineraryEntry>,
    itinerary_1: &Vec<ItineraryEntry>,
) -> bool {
    let mut set_0 = HashSet::new();
    let mut set_1 = HashSet::new();

    for entry in itinerary_0 {
        set_0.insert(entry.setting.get_tuple_id());
    }
    for entry in itinerary_1 {
        set_1.insert(entry.setting.get_tuple_id());
    }

    set_0 == set_1
}

#[allow(dead_code)]
fn equivalent_itineraries(
    itinerary_0: &Vec<ItineraryEntry>,
    itinerary_1: &Vec<ItineraryEntry>,
) -> bool {
    if !identical_settings(itinerary_0, itinerary_1) {
        return false;
    }
    for entry_zero in itinerary_0 {
        for entry_one in itinerary_1 {
            if entry_zero.setting.get_tuple_id() == entry_one.setting.get_tuple_id()
                && (entry_zero.ratio - entry_one.ratio).abs() > 0.01
            {
                return false;
            }
        }
    }
    true
}

#[allow(private_bounds)]
pub trait ContextSettingExt:
    PluginContext + ContextSettingInternalExt + ContextRandomExt + ContextPeopleExt
{
    #[allow(dead_code)]
    fn get_setting_properties(
        &self,
        setting: &dyn SettingCategory,
    ) -> Result<SettingProperties, IxaError> {
        let data_container = self.get_data(SettingDataPlugin);

        match data_container
            .setting_properties
            .get(&setting.get_type_id())
        {
            None => Err(IxaError::from(
                "Attempting to get properties of unregistered setting type",
            )),
            Some(properties) => Ok(*properties),
        }
    }
    fn register_setting_category(
        &mut self,
        setting: &dyn SettingCategory,
        setting_props: SettingProperties,
    ) -> Result<(), IxaError> {
        let container = self.get_data_mut(SettingDataPlugin);

        if !container.setting_categories.insert(setting.get_type_id()) {
            return Err(IxaError::from("Setting type is already registered"));
        }

        // Add properties
        container
            .setting_properties
            .insert(setting.get_type_id(), setting_props);
        Ok(())
    }

    #[allow(dead_code)]
    fn remove_modified_itinerary_entry(
        &mut self,
        person_id: PersonId,
        itinerary_modifier: ItineraryModifiers,
    ) -> Result<(), IxaError> {
        let _span = open_span("remove_modified_itinerary");

        let container = self.get_data_mut(SettingDataPlugin);
        let ranking = match itinerary_modifier {
            ItineraryModifiers::ReplaceWith { ranking, .. }
            | ItineraryModifiers::RestrictTo { ranking, .. }
            | ItineraryModifiers::Exclude { ranking, .. } => ranking,
        };
        let previous_dominate_itinerary_ranking = container
            .get_dominate_modified_itinerary_ranking(person_id)
            .ok_or_else(|| IxaError::from("Can't find dominate modified itinerary"))?;
        if ranking == previous_dominate_itinerary_ranking {
            if let Some(previous_mod_itinerary) =
                container.get_dominate_modified_itinerary(person_id)
            {
                container.deactivate_itinerary(person_id, previous_mod_itinerary.clone());
            }
        }
        if let Some(modifier_map) = container.modified_itineraries.get_mut(&person_id) {
            if modifier_map.remove(&ranking).is_none() {
                return Err(IxaError::from(format!(
                    "Can't find modified itinerary with ranking {ranking} for person {person_id}."
                )));
            }
            if modifier_map.is_empty() {
                container.modified_itineraries.remove(&person_id);
                if let Some(default_itinerary) = container.itineraries.get(&person_id) {
                    container.activate_itinerary(person_id, &default_itinerary.clone())?;
                } else {
                    return Err(IxaError::from(
                        "Can't remove modified itinerary if there isn't a default present",
                    ));
                }
            } else {
                let new_dominant_itinerary = container
                    .get_dominate_modified_itinerary(person_id)
                    .ok_or_else(|| IxaError::from("Can't find dominate modified itinerary"))?
                    .clone();
                container.activate_itinerary(person_id, &new_dominant_itinerary)?;
            }
        }
        Ok(())
    }

    fn modify_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary_modifier: ItineraryModifiers,
    ) -> Result<(), IxaError> {
        let _span = open_span("modify_itinerary");
        match itinerary_modifier {
            ItineraryModifiers::ReplaceWith { itinerary, ranking } => {
                trace!("ItineraryModifier::Replace person {person_id} --  {itinerary:?}");

                // The model currtently assumes that people cannot change the settings of their itinerary, only the ratios of those settings.
                // We therefore assert that the settings are identical for this `ItineraryModifiers::ReplaceWith` as it is the only
                // modifier that could feasibly change settings ids between default and modified itineraries.
                assert!(identical_settings(
                    self.get_itinerary(person_id, ItinerarySelector::Default)
                        .unwrap(),
                    &itinerary
                ));
                self.add_modified_itinerary(person_id, itinerary, true, ranking)
            }
            ItineraryModifiers::RestrictTo { setting, ranking } => {
                trace!(
                    "ItineraryModifier::RestrictTo person {person_id} -- {:?}",
                    setting.get_type_id()
                );
                self.limit_itinerary_by_setting_category(person_id, setting, ranking)
            }
            ItineraryModifiers::Exclude { setting, ranking } => {
                trace!(
                    "ItineraryModifier::Exclude person {person_id}-- {:?}",
                    setting.get_type_id()
                );
                self.exclude_setting_from_itinerary(person_id, setting, ranking)
            }
        }
    }

    #[allow(dead_code)]
    /// `get_setting_ids` returns a vector of the numerical values of the ID for a setting type
    fn get_setting_ids(
        &mut self,
        person_id: PersonId,
        setting_category: &dyn SettingCategory,
    ) -> Vec<usize> {
        let container = self.get_data_mut(SettingDataPlugin);
        match container.itineraries.get(&person_id) {
            None => Vec::new(),
            Some(itineraries) => {
                let mut setting_id_vec = Vec::new();
                for entry in itineraries {
                    if entry.setting.get_type_id() == setting_category.get_type_id() {
                        setting_id_vec.push(entry.setting.id());
                    }
                }
                setting_id_vec
            }
        }
    }

    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let _span = open_span("add_itinerary");
        // Normalize itinerary ratios
        self.validate_itinerary(&itinerary)?;

        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        // If we passed validation, we know setting entries aren't all zero, so we can divide by
        // total_ratio without worrying about dividing by zero.
        let mut itinerary = itinerary;
        for entry in &mut itinerary {
            entry.ratio /= total_ratio;
        }
        let container = self.get_data_mut(SettingDataPlugin);

        // Clean up settings that from previous itinerary, if there is one

        if let Some(previous_itinerary) = container.itineraries.get(&person_id) {
            container.deactivate_itinerary(person_id, previous_itinerary.clone());
        }

        container.activate_itinerary(person_id, &itinerary)?;
        container.itineraries.insert(person_id, itinerary);

        Ok(())
    }

    #[allow(dead_code)]
    fn get_current_itinerary(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        self.get_itinerary(person_id, ItinerarySelector::Current)
    }

    #[allow(dead_code)]
    fn get_setting_members(&self, setting: &dyn AnySettingId) -> Option<&HashSet<PersonId>> {
        self.get_setting_members_internal(setting, MembershipSelector::Active)
    }

    /// Get the total current infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// with members filtered as Active and in the Current itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of active members in the setting
    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let container = self.get_data(SettingDataPlugin);
        let mut collector = 0.0;
        container.with_itinerary(
            person_id,
            ItinerarySelector::Current,
            MembershipSelector::Active,
            |setting, setting_props, members, ratio| {
                let multiplier: f64 = if members.is_empty() {
                    0.0
                } else {
                    setting.calculate_multiplier(members, *setting_props)
                };
                collector += ratio * multiplier;
            },
        );
        collector
    }
    /// Get the maximum infectiousness multiplier for a person across all settings
    /// derived from both the default and modified itineraries of the person.
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of all active and inactive members in the setting
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let container = self.get_data(SettingDataPlugin);
        let mut collector = 0.0;
        container.with_itinerary(
            person_id,
            ItinerarySelector::Default,
            MembershipSelector::Union,
            |setting, setting_props, members, _ratio| {
                let multiplier: f64 = setting.calculate_multiplier(members, *setting_props);
                // We want to identify the max at the setting level, not itinerary level, so that we sample at the true maximum possible rate
                collector = f64::max(collector, multiplier);
            },
        );
        collector
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: &dyn AnySettingId,
    ) -> Result<Option<PersonId>, IxaError> {
        let _span = open_span("get_contact");
        if let Some(members) =
            self.get_setting_members_internal(setting, MembershipSelector::Active)
        {
            if members.get(&person_id).is_some() && members.len() == 1 {
                return Ok(None);
            }
            let mut contact_id;
            loop {
                contact_id = self.sample_active_setting_members(setting);

                if contact_id != Some(person_id) {
                    break;
                }
            }
            return Ok(contact_id);
        }
        Err(IxaError::from("Group membership is None"))
    }

    fn sample_current_setting(&self, person_id: PersonId) -> Option<&dyn AnySettingId> {
        let _span = open_span("sample_setting");
        let container = self.get_data(SettingDataPlugin);
        let mut itinerary_multiplier = Vec::new();
        container.with_itinerary(
            person_id,
            ItinerarySelector::Current,
            MembershipSelector::Active,
            |setting, setting_props, members, ratio| {
                let multiplier = if members.is_empty() {
                    0.0
                } else {
                    setting.calculate_multiplier(members, *setting_props)
                };
                itinerary_multiplier.push(ratio * multiplier);
            },
        );

        let setting_index = self.sample_weighted(SettingsRng, &itinerary_multiplier);

        if let Some(itinerary) = self.get_itinerary(person_id, ItinerarySelector::Current) {
            let itinerary_entry = &itinerary[setting_index];
            Some(itinerary_entry.setting.as_ref())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_modified_itinerary(
        &self,
        person_id: PersonId,
    ) -> Option<&HashMap<usize, Vec<ItineraryEntry>>> {
        self.get_data(SettingDataPlugin)
            .modified_itineraries
            .get(&person_id)
    }
}
impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) {
    let Params {
        settings_properties,
        ..
    } = context.get_params();

    for (setting_category, setting_properties) in settings_properties.clone() {
        match setting_category {
            CoreSettingsTypes::Home => {
                context
                    .register_setting_category(&Home, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::CensusTract => {
                context
                    .register_setting_category(&CensusTract, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::School => {
                context
                    .register_setting_category(&School, setting_properties)
                    .unwrap();
            }
            CoreSettingsTypes::Workplace => {
                context
                    .register_setting_category(&Workplace, setting_properties)
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        hospitalizations::Hospitalized,
        parameters::{GlobalParams, ItinerarySpecificationType},
        policies::previous_guidance::IsolatingStatus,
        settings::ContextSettingExt,
    };
    use ixa::{define_person_property, ContextGlobalPropertiesExt, ContextPeopleExt};
    use statrs::assert_almost_eq;
    use std::rc::Rc;

    define_setting_category!(Community);

    fn register_default_settings(context: &mut Context) {
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &Workplace,
                SettingProperties {
                    alpha: 0.3,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        context
            .register_setting_category(
                &School,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();
    }

    #[test]
    fn test_setting_category_creation() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
                SettingProperties {
                    alpha: 0.001,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.25,
                    }),
                },
            )
            .unwrap();
        let home_props = context.get_setting_properties(&Home).unwrap();
        let tract_props = context.get_setting_properties(&CensusTract).unwrap();

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
        let e = context.get_setting_properties(&Home).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Attempting to get properties of unregistered setting type"
                );
            }
            Some(ue) => panic!(
                "Expected an error setting plugin data is none. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context.get_setting_properties(&Home).unwrap();
        let e = context.get_setting_properties(&CensusTract).err();
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
    fn test_duplicate_setting_category_registration() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        let e = context
            .register_setting_category(
                &Home,
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
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
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
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 2), -0.5)];

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
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Community, 2), 0.5)];

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
    fn test_identical_settings() {
        let itinerary_0 = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 1.0),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 3), 1.5),
        ];
        let itinerary_1 = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.0),
            ItineraryEntry::new(SettingId::new(Home, 2), 1.5),
            ItineraryEntry::new(SettingId::new(Home, 3), 0.5),
        ];
        let itinerary_2 = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 1.0),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 4), 1.5),
        ];

        assert!(identical_settings(&itinerary_0, &itinerary_1));
        assert!(!identical_settings(&itinerary_0, &itinerary_2));
    }

    #[test]
    fn test_change_activity_members() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let active_person = context.add_person(()).unwrap();
        let inactive_person = context.add_person(()).unwrap();
        let active_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 1.0)];
        let inactive_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 0.0)];
        context
            .add_itinerary(active_person, active_itinerary.clone())
            .unwrap();
        context
            .add_itinerary(inactive_person, inactive_itinerary.clone())
            .unwrap();

        let home = SettingId::new(Home, 1);

        let members = context
            .get_setting_members_internal(&home, MembershipSelector::Union)
            .unwrap();
        let active_members = context
            .get_setting_members_internal(&home, MembershipSelector::Active)
            .unwrap();
        let inactive_members = context
            .get_setting_members_internal(&home, MembershipSelector::Inactive)
            .unwrap();

        assert_eq!(members.len(), 2);
        assert_eq!(active_members.len(), 1);
        assert_eq!(inactive_members.len(), 1);

        let container = context.get_data_mut(SettingDataPlugin);

        // Activate the "inactive" itinerary with ratio 0.0 for the active person
        container
            .activate_itinerary(active_person, &inactive_itinerary)
            .unwrap();

        let members = context
            .get_setting_members_internal(&home, MembershipSelector::Union)
            .unwrap();
        let active_members = context
            .get_setting_members_internal(&home, MembershipSelector::Active)
            .unwrap();
        let inactive_members = context
            .get_setting_members_internal(&home, MembershipSelector::Inactive)
            .unwrap();

        assert_eq!(members.len(), 2);
        assert_eq!(active_members.len(), 0);
        assert_eq!(inactive_members.len(), 2);

        let container = context.get_data_mut(SettingDataPlugin);
        assert_eq!(
            container.get_active_setting_members(&home).unwrap().len(),
            0
        );

        // Reactivate the "active" itinerary for the active person
        container
            .activate_itinerary(active_person, &active_itinerary)
            .unwrap();
        let container = context.get_data(SettingDataPlugin);
        assert_eq!(
            container
                .active_members
                .get(&home.get_tuple_id())
                .unwrap()
                .get(&active_person),
            Some(&0)
        );
        assert_eq!(
            container
                .active_members_vec
                .get(&home.get_tuple_id())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            container
                .active_members_vec
                .get(&home.get_tuple_id())
                .unwrap()[0],
            active_person
        );
    }

    #[test]
    fn test_add_itinerary() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
        ];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(&SettingId::new(Home, 2))
            .unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_person(()).unwrap();
        let itinerary2 = vec![ItineraryEntry::new(SettingId::new(Home, 2), 1.0)];
        context.add_itinerary(person2, itinerary2).unwrap();

        let members2 = context
            .get_setting_members(&SettingId::new(Home, 2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let itinerary3 = vec![ItineraryEntry::new(SettingId::new(Home, 3), 0.5)];
        context.add_itinerary(person, itinerary3).unwrap();
        let members2_removed = context
            .get_setting_members(&SettingId::new(Home, 2))
            .unwrap();
        assert_eq!(members2_removed.len(), 1);
        let members3 = context
            .get_setting_members(&SettingId::new(Home, 3))
            .unwrap();
        assert_eq!(members3.len(), 1);
        let members1_removed = context
            .get_setting_members(&SettingId::new(Home, 1))
            .unwrap();
        assert_eq!(members1_removed.len(), 0);
    }

    #[test]
    fn test_get_itinerary() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let default_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
        ];
        let modified_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.0),
            ItineraryEntry::new(SettingId::new(Home, 2), 1.0),
        ];
        context.add_itinerary(person, default_itinerary).unwrap();
        context
            .add_modified_itinerary(person, modified_itinerary, false, 1)
            .unwrap();

        let default = context
            .get_itinerary(person, ItinerarySelector::Default)
            .unwrap();
        let current = context
            .get_itinerary(person, ItinerarySelector::Current)
            .unwrap();

        for entry in default {
            assert_almost_eq!(entry.ratio, 0.5, 0.0);
        }
        for entry in current {
            if entry.setting.id() == 1 {
                assert_almost_eq!(entry.ratio, 0.0, 0.0);
            } else {
                assert_almost_eq!(entry.ratio, 1.0, 0.0);
            }
        }
    }

    #[test]
    fn test_get_setting_ids() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_person(()).unwrap();
        let person_two = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(Workplace, 1), 0.5),
        ];

        let itinerary_two = vec![ItineraryEntry::new(SettingId::new(Workplace, 1), 0.5)];

        context.add_itinerary(person, itinerary).unwrap();
        context.add_itinerary(person_two, itinerary_two).unwrap();

        let h_id = context.get_setting_ids(person, &Home);
        let w_id = context.get_setting_ids(person_two, &Workplace);

        let h_members = context
            .get_setting_members(&SettingId::new(Home, h_id[0]))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, w_id[0]))
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
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];
        let isolation_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.0),
        ];

        let _ = context.add_itinerary(person, itinerary);

        let members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 1);

        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::ReplaceWith {
                itinerary: isolation_itinerary,
                ranking: 1,
            },
        );

        let members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 0);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_itinerary_modifiers_replace() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let itinerary_vec: Vec<Vec<ItineraryEntry>> = vec![
            vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
                ItineraryEntry::new(SettingId::new(School, 0), 1.0),
            ],
            vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ],
            vec![
                ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ],
            vec![
                ItineraryEntry::new(SettingId::new(Home, 1), 1.0),
                ItineraryEntry::new(SettingId::new(School, 0), 1.0),
            ],
            vec![
                ItineraryEntry::new(SettingId::new(Home, 1), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ],
            vec![
                ItineraryEntry::new(SettingId::new(Home, 1), 1.0),
                ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
            ],
        ];

        let mut person_0: Option<PersonId> = None;
        for (p_id, itinerary_entries) in itinerary_vec.iter().enumerate() {
            let person = context.add_person(()).unwrap();
            let _e = context.add_itinerary(person, itinerary_entries.clone());
            if p_id == 0 {
                person_0 = Some(person);
            }
        }
        let alpha_h = context.get_setting_properties(&Home).unwrap().alpha;
        let alpha_w = context.get_setting_properties(&Workplace).unwrap().alpha;
        let alpha_s = context.get_setting_properties(&School).unwrap().alpha;

        let inf_multiplier =
            context.calculate_max_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = f64::max(
            f64::max((2_f64).powf(alpha_h), (4_f64).powf(alpha_w)),
            (1_f64).powf(alpha_s),
        );

        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.0);

        // 2. Isolate person with itinerary [(Home 0 , 0.95), (Workplace 0, 0.05)]
        let isolation_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.95),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.05),
            ItineraryEntry::new(SettingId::new(School, 0), 0.0),
        ];
        let itinerary_modifier = ItineraryModifiers::ReplaceWith {
            itinerary: isolation_itinerary.clone(),
            ranking: 1,
        };
        let _ = context.modify_itinerary(person_0.unwrap(), itinerary_modifier.clone());

        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let h_one_members = context
            .get_setting_members(&SettingId::new(Home, 1))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        let s_members = context
            .get_setting_members(&SettingId::new(School, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(h_one_members.len(), 3);
        assert_eq!(w_members.len(), 5);
        assert_eq!(s_members.len(), 1);

        let inf_multiplier =
            context.calculate_current_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = (0.95) * (2_f64).powf(alpha_h) + (0.05) * (4_f64).powf(alpha_w);
        assert_almost_eq!(inf_multiplier, expected_multiplier, 0.001);

        // 3. Remove modified itinerary; get back to normal
        let _ = context.remove_modified_itinerary_entry(person_0.unwrap(), itinerary_modifier);
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let h_one_members = context
            .get_setting_members(&SettingId::new(Home, 1))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        let s_members = context
            .get_setting_members(&SettingId::new(School, 0))
            .unwrap();

        assert_eq!(h_members.len(), 3);
        assert_eq!(h_one_members.len(), 3);
        assert_eq!(w_members.len(), 5);
        assert_eq!(s_members.len(), 2);

        let inf_multiplier =
            context.calculate_max_infectiousness_multiplier_for_person(person_0.unwrap());
        let expected_multiplier = f64::max(
            f64::max((2_f64).powf(alpha_h), (4_f64).powf(alpha_w)),
            (1_f64).powf(alpha_s),
        );

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
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (limit default): {h_members:?}");
        println!("WORK MEMBERS (limit default): {w_members:?}");

        let itinerary_modifier = ItineraryModifiers::RestrictTo {
            setting: &Home,
            ranking: 1,
        };
        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(person, itinerary_modifier.clone());

        // Check membership
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (limit isolation): {h_members:?}");
        println!("WORK MEMBERS (limit isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary_entry(person, itinerary_modifier);
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
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
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());

        for _ in 0..2 {
            let itinerary_home = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_home);
        }
        for _ in 0..2 {
            let itinerary_work = vec![ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0)];

            let p_id = context.add_person(()).unwrap();
            let _ = context.add_itinerary(p_id, itinerary_work);
        }
        // Check membership
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 3);
        println!("HOME MEMBERS (exclude default): {h_members:?}");
        println!("WORK MEMBERS (exclude default): {w_members:?}");

        let itinerary_modifier = ItineraryModifiers::Exclude {
            setting: &Workplace,
            ranking: 1,
        };
        // Reduce itinerary to only Home
        let _ = context.modify_itinerary(person, itinerary_modifier.clone());

        // Check membership
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(h_members.len(), 3);
        assert_eq!(w_members.len(), 2);
        println!("HOME MEMBERS (exclude isolation): {h_members:?}");
        println!("WORK MEMBERS (exclude isolation): {w_members:?}");

        let _ = context.remove_modified_itinerary_entry(person, itinerary_modifier);
        let h_members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
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
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
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
                    ItineraryEntry::new(SettingId::new(Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let members = context
                .get_setting_members(&SettingId::new(Home, s))
                .unwrap();
            let tract_members = context
                .get_setting_members(&SettingId::new(CensusTract, s))
                .unwrap();

            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_registration_activity() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
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
                    ItineraryEntry::new(SettingId::new(Home, s), 1.0),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.0),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let inactive_home_members = context
                .get_setting_members_internal(
                    &SettingId::new(Home, s),
                    MembershipSelector::Inactive,
                )
                .unwrap();
            let inactive_tract_members = context
                .get_setting_members_internal(
                    &SettingId::new(CensusTract, s),
                    MembershipSelector::Inactive,
                )
                .unwrap();
            let active_home_members = context
                .get_setting_members_internal(&SettingId::new(Home, s), MembershipSelector::Active)
                .unwrap();
            let active_tract_members = context.get_setting_members_internal(
                &SettingId::new(CensusTract, s),
                MembershipSelector::Active,
            );
            let all_home_members = context
                .get_setting_members_internal(&SettingId::new(Home, s), MembershipSelector::Union)
                .unwrap();
            let all_tract_members = context
                .get_setting_members_internal(
                    &SettingId::new(CensusTract, s),
                    MembershipSelector::Union,
                )
                .unwrap();

            assert!(inactive_home_members.is_empty());
            assert_eq!(inactive_tract_members.len(), 5);
            assert_eq!(active_home_members.len(), 5);
            assert!(active_tract_members.is_none());
            assert_eq!(all_home_members.len(), 5);
            assert_eq!(all_tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_multiplier() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
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
                let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, s), 0.5)];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }

        let home_id = 0;
        let person = context.add_person(()).unwrap();
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, home_id), 0.5)];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(&SettingId::new(Home, home_id))
            .unwrap();

        let setting_type = &SettingId::new(Home, home_id);

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
                    ItineraryEntry::new(SettingId::new(Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        let person = context.add_person(()).unwrap();
        context.add_itinerary(person, itinerary).unwrap();

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context.calculate_max_infectiousness_multiplier_for_person(person);
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);

        // If person's itinerary is changed for two settings,
        // CensusTract 0 should have 6 members, Home 0 should have 7 members
        // the total infectiousness should be the sum of infs * proportion
        let person = context.add_person(()).unwrap();
        let itinerary_complete = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        context.add_itinerary(person, itinerary_complete).unwrap();
        let members_home = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let members_tract = context
            .get_setting_members(&SettingId::new(CensusTract, 0))
            .unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings =
            context.calculate_max_infectiousness_multiplier_for_person(person);

        let alpha_h = context.get_setting_properties(&Home).unwrap().alpha;
        let alpha_ct = context.get_setting_properties(&CensusTract).unwrap().alpha;

        assert_almost_eq!(
            inf_multiplier_two_settings,
            f64::max(
                f64::from(7 - 1).powf(alpha_h),
                f64::from(6 - 1).powf(alpha_ct)
            ),
            0.0
        );
    }

    #[test]
    fn test_sample_setting() {
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();

        // When person a is used to select a setting for contact, it should return Home. While they are
        // also a member of CensusTract, since they are the only member the multiplier used to weight the
        // selection is 0.0 from calculate_multiplier. Thus the probability CensusTract is selected is 0.0.
        let setting_id = context.sample_current_setting(person_a).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id(), 0);

        let setting_id = context.sample_current_setting(person_b).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id(), 0);

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();
        let setting_id = context.sample_current_setting(person_c).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<CensusTract>());
        assert_eq!(setting_id.id(), 0);
    }

    #[test]
    fn test_get_contact_from_setting() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: None,
                },
            )
            .unwrap();

        let person_a = context.add_person(()).unwrap();
        let person_b = context.add_person(()).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();
        let setting_id = context.sample_current_setting(person_a).unwrap();
        let members = context.get_setting_members(setting_id).unwrap();
        assert!(members.contains(&person_a));

        assert_eq!(
            Some(person_b),
            context
                .sample_from_setting_with_exclusion(person_a, setting_id)
                .unwrap()
        );

        assert!(context
            .sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 0))
            .unwrap()
            .is_none());

        let person_c = context.add_person(()).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();

        let e =
            context.sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 10));
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

    define_person_property!(Age, usize);

    #[test]
    fn test_itinerary_specification_none() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: None,
                },
            )
            .unwrap();
        let e = get_itinerary_ratio(&context, &SettingId::new(Home, 0)).err();
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
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &School,
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
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
        assert_eq!(itinerary.len(), 1);
        assert_eq!(itinerary[0].setting.get_type_id(), TypeId::of::<Home>());
        assert_eq!(itinerary[0].setting.id(), 1);
        assert_almost_eq!(itinerary[0].ratio, 0.5, 0.0);

        // Test appending an entry with a different setting type
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 42), None).unwrap();
        assert_eq!(itinerary.len(), 2);
        assert_eq!(itinerary[1].setting.get_type_id(), TypeId::of::<School>());
        assert_eq!(itinerary[1].setting.id(), 42);
        assert_almost_eq!(itinerary[1].ratio, 0.25, 0.0);

        // Test appending an entry with a non-default ratio
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 2), Some(1.0))
            .unwrap();
        assert_eq!(itinerary.len(), 3);
        assert_eq!(itinerary[2].setting.get_type_id(), TypeId::of::<Home>());
        assert_eq!(itinerary[2].setting.id(), 2);
        assert_almost_eq!(itinerary[2].ratio, 1.0, 0.0);
    }

    #[test]
    fn test_get_itinerary_ratio() {
        let mut context = Context::new();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();

        // Test with a valid setting type
        let ratio = get_itinerary_ratio(&context, &SettingId::new(Home, 0)).unwrap();
        assert_almost_eq!(ratio, 0.5, 0.0);
    }

    #[test]
    fn test_only_include_registered_settings_in_itineraries() {
        let mut context = Context::new();
        let parameters = Params {
            settings_properties: HashMap::from([(
                CoreSettingsTypes::Home,
                SettingProperties {
                    alpha: 0.5,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )]),
            ..Default::default()
        };

        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        init(&mut context);
        let mut iitinerary = vec![];
        append_itinerary_entry(
            &mut iitinerary,
            &context,
            SettingId::new(Workplace, 1),
            None,
        )
        .unwrap();

        assert_eq!(iitinerary.len(), 0);

        append_itinerary_entry(&mut iitinerary, &context, SettingId::new(Home, 1), None).unwrap();
        assert_eq!(iitinerary.len(), 1);
        assert_eq!(iitinerary[0].setting.get_type_id(), TypeId::of::<Home>());
    }

    #[test]
    fn test_itinerary_normalized_one() {
        let mut context = Context::new();
        let person = context.add_person(()).unwrap();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 0.1,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 5.0,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &CensusTract,
                SettingProperties {
                    alpha: 0.01,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 2.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &School,
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
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
        append_itinerary_entry(
            &mut itinerary,
            &context,
            SettingId::new(CensusTract, 1),
            None,
        )
        .unwrap();
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 1), None).unwrap();

        context.add_itinerary(person, itinerary).unwrap();
        let itinerary = context
            .get_itinerary(person, ItinerarySelector::Current)
            .unwrap();

        let total_ratio: Vec<f64> = itinerary.iter().map(|entry| entry.ratio).collect();
        assert_eq!(total_ratio, vec![0.5, 0.25, 0.25]);
    }
    #[test]
    fn test_multiple_modified_itineraries() {
        let mut context = Context::new();
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];
        let isolation_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.0),
        ];
        let work_from_home_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.75),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.25),
        ];

        let _ = context.add_itinerary(person, itinerary);

        let members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 1);

        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::ReplaceWith {
                itinerary: isolation_itinerary,
                ranking: 1,
            },
        );

        let members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 0);
        assert_eq!(context.get_modified_itinerary(person).unwrap().len(), 1);

        let _ = context.modify_itinerary(
            person,
            ItineraryModifiers::ReplaceWith {
                itinerary: work_from_home_itinerary,
                ranking: 2,
            },
        );

        let members = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let w_members = context
            .get_setting_members(&SettingId::new(Workplace, 0))
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(w_members.len(), 0);
        assert_eq!(context.get_modified_itinerary(person).unwrap().len(), 2);
    }

    #[test]
    fn test_itinerary_modifier_ranking_person_properties() {
        let mut context = Context::new();
        register_default_settings(&mut context);

        let person = context.add_person(()).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0 / 3.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0 / 3.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0 / 3.0),
        ];

        let isolating_itinerary = Rc::new(vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.5),
        ]);

        let hospitalized_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.0),
        ];

        let _ = context.add_itinerary(person, itinerary.clone());
        context
            .store_and_subscribe_itinerary_modifier_values(
                IsolatingStatus,
                &[(
                    true,
                    ItineraryModifiers::Exclude {
                        setting: &CensusTract,
                        ranking: 2,
                    },
                )],
            )
            .unwrap();
        context
            .store_and_subscribe_itinerary_modifier_values(
                Hospitalized,
                &[(
                    true,
                    ItineraryModifiers::RestrictTo {
                        setting: &Home,
                        ranking: 1,
                    },
                )],
            )
            .unwrap();
        context.add_plan(1.0, move |ctx| {
            ctx.set_person_property(person, IsolatingStatus, true);
        });

        let isolating_itinerary_clone = isolating_itinerary.clone();
        context.add_plan(1.0, move |ctx| {
            assert!(equivalent_itineraries(
                ctx.get_current_itinerary(person).unwrap(),
                &isolating_itinerary_clone
            ));
        });

        context.add_plan(1.0, move |ctx| {
            ctx.set_person_property(person, Hospitalized, true);
        });

        context.add_plan(1.0, move |ctx| {
            assert!(equivalent_itineraries(
                ctx.get_current_itinerary(person).unwrap(),
                &hospitalized_itinerary
            ));
        });

        context.add_plan(1.0, move |ctx| {
            ctx.set_person_property(person, Hospitalized, false);
        });
        let isolating_itinerary_clone = isolating_itinerary.clone();
        context.add_plan(1.0, move |ctx| {
            assert!(equivalent_itineraries(
                ctx.get_current_itinerary(person).unwrap(),
                &isolating_itinerary_clone
            ));
        });

        context.add_plan(1.0, move |ctx| {
            ctx.set_person_property(person, IsolatingStatus, false);
        });

        context.add_plan(1.0, move |ctx| {
            assert!(equivalent_itineraries(
                ctx.get_current_itinerary(person).unwrap(),
                &itinerary
            ));
        });

        context.execute();
    }
}
