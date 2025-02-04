use ixa::{define_rng, Context, ContextPeopleExt, PersonId};

use crate::{
    infection_propagation_loop::{InfectionStatus, InfectionStatusValue},
    population_loader::{Alive, HouseholdSettingId},
};

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
    fn get_contact(
        &mut self,
        transmitter_id: PersonId,
        household_id: usize,
    ) -> Option<PersonId>;
}

impl ContextContactExt for Context {
    fn get_contact(
        &mut self,
        transmitter_id: PersonId,
        household_id: usize,
    ) -> Option<PersonId> {
        // Get list of eligible people (for now, all alive people). May be expanded in the future
        // to instead be list of alive people in the transmitter's contact setting or household.
        // We sample a random person from this list.
        let people = self.query_people((Alive, true));

        if people.len() > 1 {
            let mut contact_id = transmitter_id;
            // Continue sampling until we get a contact_id that's not the transmitter_id.
            while contact_id == transmitter_id {
                contact_id = match self.sample_person(
                    ContactRng,
                    (
                        (Alive, true),
                        // I'm not thrilled about directly referencing an internal person property
                        (HouseholdSettingId, household_id),
                        (InfectionStatus, InfectionStatusValue::Susceptible),
                    ),
                ) {
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
mod test {
    use super::ContextContactExt;
    use crate::population_loader::{Alive, HouseholdSettingId};
    use ixa::{Context, ContextPeopleExt, ContextRandomExt, IxaError};

    #[test]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        let transmitter = context.add_person((HouseholdSettingId, 1)).unwrap();
        let result = context.get_contact(transmitter, 1);
        assert!(result.is_none());
    }

    #[test]
    fn test_return_none() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((HouseholdSettingId, 1)).unwrap();
        let _ = context.add_person((Alive, false)).unwrap();
        let observed_contact = context.get_contact(transmitter, 1);
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_remaining_alive_person() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitter = context.add_person((HouseholdSettingId, 1)).unwrap();
        let _ = context.add_person(((Alive, false), (HouseholdSettingId, 1))).unwrap();
        let presumed_contact = context.add_person((HouseholdSettingId, 1)).unwrap();
        let observed_contact = context.get_contact(transmitter, 1).unwrap();
        assert_eq!(observed_contact, presumed_contact);
    }
}
