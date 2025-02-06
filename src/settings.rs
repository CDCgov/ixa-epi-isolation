use crate::contact::ContextContactExt;
use ixa::{
    people::{Query, QueryAnd},
    Context, ContextPeopleExt, PersonId, PersonProperty,
};

// TODO<ryl8@cdc.gov> This is usize for now, but it could be a real type
type SettingId = usize;
pub trait Setting: PersonProperty<Value = SettingId> + 'static + Copy + Clone {
    fn get_name(&self) -> String;
}

#[macro_export]
macro_rules! define_setting {
    ($name:ident) => {
        paste::paste! {
            ixa::define_person_property!($name, usize);
            impl $crate::settings::Setting for $name {
                fn get_name(&self) -> String {
                    stringify!($name).to_string()
                }
            }
        }
    };
}

pub trait ContextSettingExt {
    /// Get a person's setting identifier
    fn get_person_setting_id<S: Setting>(&self, person_id: PersonId, setting: S) -> usize;
    // Set a person's setting identifier
    fn set_person_setting_id<S: Setting>(
        &mut self,
        person_id: PersonId,
        setting: S,
        setting_id: SettingId,
    );
    /// Return all members of the setting; you can pass a query to filter members
    /// with additional properties, fo example `(Alive, true)`
    fn get_setting_members<S: Setting, Q: Query>(
        &self,
        _setting: S,
        setting_id: SettingId,
        query: Q,
    ) -> Vec<PersonId>;
    /// Get a contact for a particular setting (other than the person themselves)
    fn get_contact_from_setting<S: Setting, Q: Query + Clone>(
        &self,
        person_id: PersonId,
        setting: S,
        q: Q,
    ) -> Option<PersonId>;
}

impl ContextSettingExt for Context {
    fn get_person_setting_id<S: Setting>(&self, person_id: PersonId, _setting: S) -> SettingId {
        self.get_person_property(person_id, S::get_instance())
    }
    fn set_person_setting_id<S: Setting>(
        &mut self,
        person_id: PersonId,
        _setting: S,
        setting_id: SettingId,
    ) {
        self.set_person_property(person_id, S::get_instance(), setting_id);
    }
    fn get_setting_members<S: Setting, Q: Query>(
        &self,
        _setting: S,
        setting_id: SettingId,
        query: Q,
    ) -> Vec<PersonId> {
        self.query_people(QueryAnd::new((S::get_instance(), setting_id), query))
    }
    fn get_contact_from_setting<S: Setting, Q: Query>(
        &self,
        person_id: PersonId,
        _setting: S,
        q: Q,
    ) -> Option<PersonId> {
        let setting_id = self.get_person_setting_id(person_id, S::get_instance());
        self.get_contact(person_id, QueryAnd::new((S::get_instance(), setting_id), q))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixa::define_person_property_with_default;
    use ixa::define_rng;
    use ixa::Context;
    use ixa::ContextPeopleExt;
    use ixa::ContextRandomExt;

    define_rng!(TestSettingRng);
    define_setting!(Group);
    define_person_property_with_default!(Alive, bool, true);

    #[test]
    fn test_get_name() {
        assert_eq!(Group.get_name(), "Group");
    }

    #[test]
    fn test_person_get_set_setting() {
        let mut ctx = ixa::Context::new();
        let p = ctx.add_person((Group, 1)).unwrap();

        // Trait extension
        assert_eq!(ctx.get_person_setting_id(p, Group), 1);

        // Set person 1’s group setting to 10 and verify it.
        ctx.set_person_setting_id(p, Group, 10);
        assert_eq!(ctx.get_person_setting_id(p, Group), 10);
    }

    #[test]
    fn test_get_members() {
        let mut ctx = ixa::Context::new();
        let p1 = ctx.add_person((Group, 5)).unwrap();
        let p2 = ctx.add_person((Group, 5)).unwrap();
        ctx.add_person((Group, 10)).unwrap();

        // Query for members of group 5.
        let members = ctx.get_setting_members(Group, 5, ());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&p1));
        assert!(members.contains(&p2));
    }

    #[test]
    fn test_return_setting_contact_none_when_only_contact() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Group, 1)).unwrap();
        context.add_person(((Alive, false), (Group, 1))).unwrap();
        let observed_contact = context.get_contact_from_setting(transmitter, Group, (Alive, true));
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_setting_contact() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Group, 1)).unwrap();
        let expected = context.add_person(((Alive, true), (Group, 1))).unwrap();
        context.add_person(((Alive, false), (Group, 1))).unwrap();
        context.add_person(((Alive, true), (Group, 2))).unwrap();
        let observed_contact = context.get_contact_from_setting(transmitter, Group, (Alive, true));
        assert_eq!(observed_contact, Some(expected));
    }
}
