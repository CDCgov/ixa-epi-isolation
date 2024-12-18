use ixa::{
    Context,
    define_rng,
    IxaError,
    ContextPeopleExt, PersonId,
    ContextRandomExt,
};

use crate::population_loader::Alive;

define_rng!(ContactRng);

pub trait ContextContactExt {
    /// Returns a potential contact for the transmitter.
    /// Returns Ok(None) if there are no eligible contacts.
    /// In the future, this function can be expanded to return a
    /// contact specific to the person's household or to weight
    /// drawing contacts be a contact matrix.
    ///
    /// Errors
    /// - If there is only one person in the population.
    fn get_contact(&mut self, transmitter_id: PersonId) -> Result<Option<PersonId>, IxaError>;
}

impl ContextContactExt for Context {
    fn get_contact(&mut self, transmitter_id: PersonId) -> Result<Option<PersonId>, IxaError> {
        if self.get_current_population() == 1 {
            return Err(IxaError::IxaError(
                "Cannot get a contact when there is only one person in the population.".to_string(),
            ));
        };
        // Get list of eligible people (for now, all alive people). May be expanded in the future
        // to instead be list of alive people in the transmitter's contact setting or household.
        // We sample a random person from this list.
        let eligible_contacts = self.query_people((Alive, true));
        if eligible_contacts.len() > 1 {
            let mut contact_id = transmitter_id;
            while contact_id == transmitter_id {
                // In the future, we might like to sample people from the list by weights according
                // to some contact matrix. We would use sample_weighted instead. We would calculate
                // the weights _before_ the loop and then sample from the list of people like here.
                contact_id =
                    eligible_contacts[self.sample_range(ContactRng, 0..eligible_contacts.len())];
            }
            Ok(Some(contact_id))
        } else {
            // This means that there are no eligible contacts in the population besides the transmitter.
            Ok(None)
        }
    }
}

#[cfg(test)]
mod test {
    use super::ContextContactExt;
    use crate::population_loader::Alive;
    use ixa::{Context, ContextPeopleExt, ContextRandomExt, IxaError};

    #[test]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        let transmitter = context.add_person(()).unwrap();
        let e = context.get_contact(transmitter);
        match e {
            Err(IxaError::IxaError(msg)) => assert_eq!(msg, "Cannot get a contact when there is only one person in the population.".to_string()),
            Err(ue) => panic!("Expected an error that there should be no contacts when there is only one person in the population. Instead got {:?}", ue.to_string()),
            Ok(Some(contact)) => panic!("Expected an error. Instead, got {contact:?} as valid contact."),
            Ok(None) => panic!("Expected an error. Instead, returned None, meaning that there are no valid contacts."),
        }
    }

    #[test]
    fn test_return_none() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person(()).unwrap();
        let _ = context.add_person((Alive, false)).unwrap();
        let observed_contact = context.get_contact(transmitter).unwrap();
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_remaining_alive_person() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person(()).unwrap();
        let _ = context.add_person((Alive, false)).unwrap();
        let presumed_contact = context.add_person(()).unwrap();
        let observed_contact = context.get_contact(transmitter).unwrap();
        assert_eq!(observed_contact.unwrap(), presumed_contact);
    }
}
