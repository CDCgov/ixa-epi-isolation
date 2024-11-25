use ixa::{
    context::Context,
    define_rng,
    error::IxaError,
    people::{ContextPeopleExt, PersonId},
    random::ContextRandomExt,
};
use statrs::distribution::Categorical;

use crate::population_loader::Alive;

define_rng!(ContactRng);

pub trait QueryContacts {
    /// returns an arbitrary contact for the transmitee
    /// a modeler in the future may institute a more complex contact model
    /// setting weights by contact setting or shared household or the like in this method
    fn get_contact(&mut self, transmitee: PersonId) -> Result<Option<PersonId>, IxaError>;
    /// samples a person from a list of people with a weight by person
    /// does not check any characteristics about the people
    /// this is a generic method that is really just used for help sampling the person id
    /// it is exposed to the user to allow them to make arbitrary sampling decisions
    fn sample_person_from_list(&mut self, list: Vec<PersonId>, weights: &[f64])
        -> Option<PersonId>;
}

impl QueryContacts for Context {
    fn get_contact(&mut self, transmitee_id: PersonId) -> Result<Option<PersonId>, IxaError> {
        if self.get_current_population() == 1 {
            return Err(IxaError::IxaError(
                "Cannot get a contact when there is only one person in the population.".to_string(),
            ));
        };
        // get list of alive people
        let alive_people = self.query_people((Alive, true));
        if alive_people.len() > 1 {
            // get a random person from the list of alive people
            // assign weights to each person in the list
            let mut weights = vec![1.0; alive_people.len()];
            // get index of transmitee
            let transmitee_index = alive_people
                .iter()
                .position(|&x| x == transmitee_id)
                .unwrap();
            // set this weight equal to 0 so we don't select the transmitee as the contact
            weights[transmitee_index] = 0.0;
            let contact_id = self.sample_person_from_list(alive_people, &weights);
            Ok(contact_id)
        } else {
            // there are no contacts in the population -- the one remaining alive person must be the transmitee
            // or there are no alive people in the population
            Ok(None)
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    fn sample_person_from_list(
        &mut self,
        list: Vec<PersonId>,
        weights: &[f64],
    ) -> Option<PersonId> {
        // subtract one because the weights are 1-indexed
        let index = self.sample_distr(ContactRng, Categorical::new(weights).unwrap());
        Some(list[index as usize])
    }
}

#[cfg(test)]
mod test {
    use super::QueryContacts;
    use crate::population_loader::Alive;
    use ixa::{context::Context, people::ContextPeopleExt, random::ContextRandomExt};

    #[test]
    #[should_panic(
        expected = "Cannot get a contact when there is only one person in the population."
    )]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        let transmitee = context.add_person(()).unwrap();
        let _ = context.get_contact(transmitee).unwrap();
    }

    #[test]
    fn test_return_none() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitee = context.add_person(()).unwrap();
        let _ = context.add_person((Alive, false)).unwrap();
        let observed_contact = context.get_contact(transmitee).unwrap();
        assert!(observed_contact.is_none());
    }

    #[test]
    fn test_return_remaining_alive_person() {
        let mut context = Context::new();
        context.init_random(108);
        let transmitee = context.add_person(()).unwrap();
        let _ = context.add_person((Alive, false)).unwrap();
        let presumed_contact = context.add_person(()).unwrap();
        let observed_contact = context.get_contact(transmitee).unwrap();
        assert_eq!(observed_contact.unwrap(), presumed_contact);
    }
}
