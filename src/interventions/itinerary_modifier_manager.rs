use ixa::{
    define_data_plugin, trace, Context, ContextPeopleExt, IxaError, PersonId, PersonProperty,
    PersonPropertyChangeEvent, PluginContext,
};
use std::{any::TypeId, collections::HashMap};

use crate::settings::{ContextSettingExt, ItineraryModifiers};

/// Defines a itinerary modifier that is used to modify the active itineraries
/// of a person based on their person properties.
// We require `Debug` for easy logging of the trait so the user can see what is happening.
pub trait ItineraryModifier: std::fmt::Debug {
    /// Return the itinerary of a person based on their properties and the current context.
    fn get_itinerary(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<&ItineraryModifiers<'_>>;

    /// For debugging purposes. The name of the itinerary modifier. The default implementation
    /// returns the `Debug` representation of the itinerary modifier struct on which this trait
    /// is implemented.
    fn get_name(&self) -> String {
        format!("{self:?}")
    }
}

// A type alias for the type of the itinerary modifiers specified via a hashmap of person
// property values and itinerary -- i.e., `modifier_key: &[(P::Value, Vec<ItineraryEntry>)]`
type PersonPropertyModifier<'a, P> = (
    P,
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    HashMap<<P as PersonProperty>::Value, ItineraryModifiers<'static>>,
);

#[allow(dead_code)]
impl<P> ItineraryModifier for PersonPropertyModifier<'_, P>
where
    P: PersonProperty + std::fmt::Debug + 'static,
    P::Value: std::hash::Hash + Eq,
{
    fn get_itinerary(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<&ItineraryModifiers<'_>> {
        let (person_property, modifier_map) = self;
        let property_val = context.get_person_property(person_id, *person_property);
        match modifier_map.get(&property_val) {
            Some(value) => Some(value),
            None => None,
        }
    }
    // fn set_itinerary(&self, context: &mut Context, person_id: PersonId, itinerary: ItineraryModifiers<'_>) -> Result<(), IxaError> {
    //     context.modify_itinerary(person_id, itinerary)
    // }
    fn get_name(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[derive(Default)]
struct ItineraryModifierContainer {
    itinerary_modifier_map: HashMap<TypeId, Box<dyn ItineraryModifier>>,
}

define_data_plugin!(
    ItineraryModifierPlugin,
    ItineraryModifierContainer,
    ItineraryModifierContainer::default()
);

pub trait ContextItineraryModifierExt: PluginContext {
    /// Register a generic itinerary modifier.
    fn register_itinerary_modifier_fn<T: ItineraryModifier + 'static>(
        &mut self,
        itinerary_modifier: T,
    ) {
        // Box the itinerary modifier to store it in the map
        // Itinerary modifiers must implement debug so that we can more easily log their addition
        let name = itinerary_modifier.get_name();
        let boxed_itinerary_modifier = Box::new(itinerary_modifier);

        // Insert the boxed function into the itinerary modifier map, using entry to handle unititialized keys
        if let Some(_modifier_fxn) = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .insert(TypeId::of::<T>(), boxed_itinerary_modifier)
        {
            trace!("Overwriting existing itinerary modifier function for and itinerary modifier {name}");
        }
    }

    fn subscriptions_for_itinerary_modifier_changes<P>(
        &mut self,
        _person_property: P,
        modifier_map: HashMap<P::Value, ItineraryModifiers<'static>>,
    ) where
        P: PersonProperty + std::fmt::Debug + 'static,
        P::Value: Eq + std::hash::Hash,
    {
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<P>| {
            if let Some(itinerary_modifiers) = modifier_map.get(&event.current) {
                context
                    .modify_itinerary(event.person_id, itinerary_modifiers.clone())
                    .unwrap();
            }
        });
    }

    /// Register an itinerary modifier that depends solely on the value of one person property.
    /// The function accepts a itinerary key, which is a slice of tuples that
    /// associate values of a specified person property with itinerary of
    /// a person with that property value.
    ///
    /// Internally, this method registers a itinerary modifier function that returns the itinerary
    /// value associated the person's property value in the itinerary key.
    #[allow(dead_code)]
    fn store_and_subscribe_itinerary_modifier_values<
        P: PersonProperty + std::fmt::Debug + 'static,
    >(
        &mut self,
        person_property: P,
        itinerary_key: &[(P::Value, ItineraryModifiers<'static>)],
    ) -> Result<(), IxaError>
    where
        P::Value: std::hash::Hash + Eq,
    {
        // Convert modifiers to HashMap
        let mut modifier_map = HashMap::new();
        for &(key, ref value) in itinerary_key {
            if let Some(old_value) = modifier_map.insert(key, (*value).clone()) {
                return Err(IxaError::IxaError(
                    "Duplicate values provided in modifier key ".to_string()
                        + &format!("Values {old_value:?} and {value:?} were both attempted to be registered to key {person_property:?}::{key:?}"),
                ));
            }
        }

        // Register a default function to simply map floats with T::Values
        self.register_itinerary_modifier_fn((person_property, modifier_map.clone()));
        self.subscriptions_for_itinerary_modifier_changes(person_property, modifier_map);

        Ok(())
    }

    /// Get the active itinerary for a person
    /// based on their set of person properties based on all registered modifiers. Queries all registered
    /// modifier functions and evaluates them based on the person's properties. Multiplies them
    /// together to get the total relative transmission modifier for the person.
    /// Returns 1.0 if no modifiers are registered for the person's infection status.
    #[allow(dead_code)]
    fn active_itinerary_modifier(&mut self, person_id: PersonId) -> Option<ItineraryModifiers<'_>>;
}

impl ContextItineraryModifierExt for Context {
    fn active_itinerary_modifier(&mut self, person_id: PersonId) -> Option<ItineraryModifiers<'_>> {
        let itinerary_modifier_plugin = self.get_data(ItineraryModifierPlugin);
        let itinerary_modifier_map = &itinerary_modifier_plugin.itinerary_modifier_map;
        // Calculate the relative modifier for each registered function and multiply them
        // together to get the total relative transmission modifier for the person
        let mut final_itinerary_modifier: ItineraryModifiers =
            ItineraryModifiers::ReplaceWith { itinerary: vec![] };
        for value in itinerary_modifier_map {
            final_itinerary_modifier = value.1.get_itinerary(self, person_id).unwrap().clone();
        }
        Some(final_itinerary_modifier)
    }
}

#[cfg(test)]
mod test {
    use ixa::{
        assert_almost_eq, define_person_property, Context, ContextGlobalPropertiesExt,
        ContextPeopleExt, ContextRandomExt,
    };
    use serde::{Deserialize, Serialize};

    use super::ItineraryModifierPlugin;
    use crate::{
        interventions::ContextItineraryModifierExt,
        parameters::{
            CoreSettingsTypes, FacemaskParameters, GlobalParams, ItinerarySpecificationType,
            Params, ProgressionLibraryType, RateFnType,
        },
        rate_fns::load_rate_fns,
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, ItineraryModifiers, SettingId,
            SettingProperties, Workplace,
        },
    };
    use std::{collections::HashMap, path::PathBuf};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum MandatoryIntervention {
        Partial,
        Full,
        NoEffect,
    }
    define_person_property!(MandatoryInterventionStatus, MandatoryIntervention);

    #[allow(dead_code)]
    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            initial_recovered: 0.35,
            proportion_asymptomatic: 0.3,
            max_time: 100.0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_empirical_rate_fns.csv"),
                scale: 0.05,
            },
            symptom_progression_library: Some(ProgressionLibraryType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_symptom_parameters.csv"),
            }),
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::Workplace,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::CensusTract,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
            ]),
            guidance_policy: None,
            facemask_parameters: Some(FacemaskParameters {
                facemask_efficacy: 0.8,
            }),
            ..Default::default()
        };
        context.init_random(1);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        crate::settings::init(&mut context);
        context
    }

    define_person_property!(Age, usize);

    #[test]
    fn test_register_modifier_values() {
        let mut context = setup();
        let full_itinerary = ItineraryModifiers::RestrictTo { setting: &Home };
        let partial_itinerary = ItineraryModifiers::Exclude {
            setting: &CensusTract,
        };
        context
            .store_and_subscribe_itinerary_modifier_values(
                MandatoryInterventionStatus,
                &[
                    (MandatoryIntervention::Partial, partial_itinerary.clone()),
                    (MandatoryIntervention::Full, full_itinerary.clone()),
                ],
            )
            .unwrap();

        // Add people with different intervention statuses
        let partial_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();
        let full_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();
        let none_id = context
            .add_person((MandatoryInterventionStatus, MandatoryIntervention::NoEffect))
            .unwrap();

        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0),
        ];
        context
            .add_itinerary(partial_id, itinerary.clone())
            .unwrap();
        context.add_itinerary(full_id, itinerary.clone()).unwrap();
        context.add_itinerary(none_id, itinerary.clone()).unwrap();

        context.add_plan(0.0, move |ctx| {
            ctx.set_person_property(
                full_id,
                MandatoryInterventionStatus,
                MandatoryIntervention::Full,
            );
            ctx.set_person_property(
                partial_id,
                MandatoryInterventionStatus,
                MandatoryIntervention::Partial,
            );
        });
        context.execute();

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context.get_data(ItineraryModifierPlugin);
        let modifier_map = &modifier_container.itinerary_modifier_map;

        // Check that the modifier map contains the expected values
        assert_eq!(modifier_map.len(), 1);

        let full_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.0),
        ];

        let partial_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 0.5),
        ];

        let none_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 1.0 / 3.0),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0 / 3.0),
            ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0 / 3.0),
        ];

        let full = context.get_current_itinerary(full_id).unwrap();

        let partial = context.get_current_itinerary(partial_id).unwrap();

        let none = context.get_current_itinerary(none_id).unwrap();

        for itinerary in full {
            for entry in &full_itinerary {
                if entry.setting.get_tuple_id() == itinerary.setting.get_tuple_id() {
                    assert_almost_eq!(entry.ratio, itinerary.ratio, 0.01);
                }
            }
        }

        for itinerary in partial {
            for entry in &partial_itinerary {
                if entry.setting.get_tuple_id() == itinerary.setting.get_tuple_id() {
                    assert_almost_eq!(entry.ratio, itinerary.ratio, 0.01);
                }
            }
        }
        for itinerary in none {
            for entry in &none_itinerary {
                if entry.setting.get_tuple_id() == itinerary.setting.get_tuple_id() {
                    assert_almost_eq!(entry.ratio, itinerary.ratio, 0.01);
                }
            }
        }
    }
}
