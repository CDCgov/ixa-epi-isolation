use ixa::{define_rng, people::Query, Context, PersonId, RngId};
use rand::Rng;

pub trait Setting {
    fn get_name(&self) -> String;
    fn get_members<Q>(&self, context: &Context, setting_id: usize, q: Q) -> Vec<PersonId>
    where
        Q: Query;
    /// Sample a random member in the setting
    /// # Errors
    /// The setting is empty
    fn sample_members<Q, R: RngId + 'static>(
        &self,
        context: &Context,
        rng: R,
        setting_id: usize,
        q: Q,
    ) -> Result<ixa::PersonId, ixa::IxaError>
    where
        Q: Query,
        R::RngType: Rng;
    fn get_person_setting_id(&self, context: &Context, person_id: PersonId) -> usize;
    fn set_person_setting_id(&self, context: &mut Context, person_id: PersonId, setting_id: usize);
}

#[macro_export]
macro_rules! define_setting {
    ($name:ident) => {
        paste::paste! {
            ixa::define_person_property!( [<$name SettingId>], usize);

            #[derive(Debug, Clone, Copy)]
            pub struct $name;
            impl $crate::settings::Setting for $name {
                fn get_name(&self) -> String {
                    stringify!($name).to_string()
                }
                fn get_members<Q>(&self, context: &Context, setting_id: usize, q: Q) -> Vec<ixa::PersonId>
                    where Q: ixa::people::Query {
                    context.query_people(ixa::people::QueryAnd::new(([<$name SettingId>], setting_id), q))
                }
                fn sample_members<Q, R: ixa::RngId + 'static>(&self, context: &Context, rng: R, setting_id: usize, q: Q) -> Result<ixa::PersonId, ixa::IxaError>
                where Q: ixa::people::Query, R::RngType: rand::Rng {
                    context.sample_person(rng, ixa::people::QueryAnd::new(([<$name SettingId>], setting_id), q))
            }
                fn get_person_setting_id(&self, context: &Context, person_id: ixa::PersonId) -> usize {
                    context.get_person_property(person_id, [<$name SettingId>])
                }
                fn set_person_setting_id(&self, context: &mut Context, person_id: ixa::PersonId, setting_id: usize) {
                    context.set_person_property(person_id, [<$name SettingId>], setting_id);
                }
            }
        }
    }
}

define_rng!(SettingContactRng);

pub trait ContextSettingExt {
    fn get_person_setting_id<T: Setting>(&self, person_id: PersonId, setting: T) -> usize;
    fn set_person_setting_id<T: Setting>(
        &mut self,
        person_id: PersonId,
        setting: T,
        setting_id: usize,
    );
    fn get_setting_members<T: Setting>(&self, setting: T, setting_id: usize) -> Vec<PersonId>;
    fn get_contact<T: Setting, Q>(&self, person_id: PersonId, setting: T, q: Q) -> Option<PersonId>
    where
        Q: Query + Clone;
}

impl ContextSettingExt for Context {
    fn get_person_setting_id<T: Setting>(&self, person_id: PersonId, setting: T) -> usize {
        setting.get_person_setting_id(self, person_id)
    }
    fn set_person_setting_id<T: Setting>(
        &mut self,
        person_id: PersonId,
        setting: T,
        setting_id: usize,
    ) {
        setting.set_person_setting_id(self, person_id, setting_id);
    }
    fn get_setting_members<S: Setting>(&self, setting: S, setting_id: usize) -> Vec<PersonId> {
        setting.get_members(self, setting_id, ())
    }
    fn get_contact<S: Setting, Q>(&self, person_id: PersonId, setting: S, q: Q) -> Option<PersonId>
    where
        Q: Query + Clone,
    {
        let setting_id = setting.get_person_setting_id(self, person_id);
        let people = setting.get_members(self, setting_id, ());

        if people.len() > 1 {
            let mut contact_id = person_id;
            // Continue sampling until we get a contact_id that's not the transmitter_id.
            while contact_id == person_id {
                contact_id =
                    match setting.sample_members(self, SettingContactRng, setting_id, q.clone()) {
                        Ok(id) => id,
                        // If sample_person errors, return None immediately.
                        Err(_) => return None,
                    };
            }
            Some(contact_id)
        } else {
            // There are no eligible contacts in the population besides the transmitter.
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixa::define_person_property_with_default;
    use ixa::Context;
    use ixa::ContextPeopleExt;
    use ixa::ContextRandomExt;

    define_rng!(TestSettingRng);
    define_setting!(Group);
    define_person_property_with_default!(Alive, bool, true);

    #[test]
    fn test_get_name() {
        let group = Group;
        assert_eq!(group.get_name(), "Group");
    }

    #[test]
    fn test_person_get_set_setting() {
        let mut ctx = ixa::Context::new();
        let p = ctx.add_person((GroupSettingId, 1)).unwrap();

        // Trait extension
        assert_eq!(ctx.get_person_setting_id(p, Group), 1);

        // Set person 1’s group setting to 10 and verify it.
        ctx.set_person_setting_id(p, Group, 10);
        assert_eq!(ctx.get_person_setting_id(p, Group), 10);
    }

    #[test]
    fn test_get_members() {
        let mut ctx = ixa::Context::new();
        let p1 = ctx.add_person((GroupSettingId, 5)).unwrap();
        let p2 = ctx.add_person((GroupSettingId, 5)).unwrap();
        ctx.add_person((GroupSettingId, 10)).unwrap();

        // Query for members of group 5.
        let members = ctx.get_setting_members(Group, 5);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&p1));
        assert!(members.contains(&p2));
    }

    #[test]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((GroupSettingId, 1)).unwrap();
        let result = context.get_contact(transmitter, Group, ());
        assert!(result.is_none());
    }

    #[test]
    fn test_return_none() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((GroupSettingId, 1)).unwrap();
        let _ = context
            .add_person(((Alive, false), (GroupSettingId, 1)))
            .unwrap();
        let observed_contact = context.get_contact(transmitter, Group, (Alive, true));
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_remaining_alive_person() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((GroupSettingId, 1)).unwrap();
        let _ = context
            .add_person(((Alive, false), (GroupSettingId, 1)))
            .unwrap();
        let presumed_contact = context.add_person((GroupSettingId, 1)).unwrap();
        let observed_contact = context
            .get_contact(transmitter, Group, (Alive, true))
            .unwrap();
        assert_eq!(observed_contact, presumed_contact);
    }
}
