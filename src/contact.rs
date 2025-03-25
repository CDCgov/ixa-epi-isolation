use ixa::{define_rng, people::Query, Context, ContextPeopleExt, ContextRandomExt, PersonId};
use crate::settings::ContextSettingExt;

define_rng!(ContactRng);

pub trait ContextContactExt {
    /// Returns a potential contact for the transmitter given a query of people
    /// properties, for example `(Alive, true)`.
    /// Returns None if there are no eligible contacts.
    #[allow(dead_code)]
    fn get_contact<Q: Query>(&self, transmitter_id: PersonId, query: Q) -> Option<PersonId>;

    /// Returns a potential contact from the transmitter give a pre-specified itinerary
    /// Returns None if there aren't eligible contacts
    // TODO: include a query
    fn get_contact_from_settings(&self, transmitter_id: PersonId) -> Option<PersonId>;
}

impl ContextContactExt for Context {
    fn get_contact<Q: Query>(&self, transmitter_id: PersonId, query: Q) -> Option<PersonId> {
        // Get list of eligible people given the provided query
        let possible_contacts = self.query_people(query);
        if possible_contacts.is_empty()
            || (possible_contacts.len() == 1 && possible_contacts[0] == transmitter_id)
        {
            return None;
        }

        // We sample a random person from the list. If the person we draw is the transmitter, we draw again.
        let mut contact_id = transmitter_id;
        while contact_id == transmitter_id {
            contact_id =
                possible_contacts[self.sample_range(ContactRng, 0..possible_contacts.len())];
        }
        Some(contact_id)
    }    
    fn get_contact_from_settings(&self, transmitter_id: PersonId) -> Option<PersonId> {
        self.draw_contact_from_itinerary(transmitter_id)
    }
}

#[cfg(test)]
mod test {
    use super::ContextContactExt;
    use ixa::{define_person_property_with_default, Context, ContextPeopleExt, ContextRandomExt};

    define_person_property_with_default!(Alive, bool, true);
    define_person_property_with_default!(IsRunner, bool, false);

    #[test]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Alive, true)).unwrap();
        let result = context.get_contact(transmitter, ());
        assert!(result.is_none());
    }

    #[test]
    fn test_one_contact_different_query() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((IsRunner, false)).unwrap();
        let contact = context.add_person((IsRunner, true)).unwrap();
        let result = context.get_contact(transmitter, (IsRunner, true));
        assert_eq!(result, Some(contact));
    }

    #[test]
    fn test_return_none_transmitter_only_contact() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Alive, true)).unwrap();
        context.add_person((Alive, false)).unwrap();
        context.add_person((Alive, false)).unwrap();

        let observed_contact = context.get_contact(transmitter, (Alive, true));
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_remaining_alive_person() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Alive, true)).unwrap();
        let presumed_contact = context.add_person((Alive, true)).unwrap();
        // Add some more people that don't match
        context.add_person((Alive, false)).unwrap();
        context.add_person((Alive, false)).unwrap();
        context.add_person((Alive, false)).unwrap();

        let observed_contact = context.get_contact(transmitter, (Alive, true)).unwrap();
        assert_eq!(observed_contact, presumed_contact);
    }
}
