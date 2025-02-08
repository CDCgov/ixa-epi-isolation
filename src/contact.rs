use ixa::{define_rng, people::Query, Context, ContextPeopleExt, PersonId};

define_rng!(ContactRng);

pub trait ContextContactExt {
    /// Returns a potential contact for the transmitter given a query of people
    /// properties, for example `(Alive, true)`.
    /// Returns None if there are no eligible contacts.
    fn get_contact<Q: Query>(&self, transmitter_id: PersonId, query: Q) -> Option<PersonId>;
}

impl ContextContactExt for Context {
    fn get_contact<Q: Query>(&self, transmitter_id: PersonId, query: Q) -> Option<PersonId> {
        // Get list of eligible people given the provided query
        // We sample a random person from this list.
        if self.query_people(query).len() > 1 {
            let mut contact_id = transmitter_id;
            while contact_id == transmitter_id {
                contact_id = match self.sample_person(ContactRng, query) {
                    Ok(id) => id,
                    Err(_) => return None,
                };
            }
            Some(contact_id)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::ContextContactExt;
    use ixa::{define_person_property_with_default, Context, ContextPeopleExt, ContextRandomExt};

    define_person_property_with_default!(Alive, bool, true);

    #[test]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((Alive, true)).unwrap();
        let result = context.get_contact(transmitter, ());
        assert!(result.is_none());
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
