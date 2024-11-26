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

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn sample_person_from_list(context: &mut Context, list: &[PersonId], weights: &[f64]) -> PersonId {
    let index = context
        .sample_distr(ContactRng, Categorical::new(weights).unwrap())
        .floor() as usize;
    list[index]
}

impl ContextContactExt for Context {
    fn get_contact(&mut self, transmitter_id: PersonId) -> Result<Option<PersonId>, IxaError> {
        if self.get_current_population() == 1 {
            return Err(IxaError::IxaError(
                "Cannot get a contact when there is only one person in the population.".to_string(),
            ));
        };
        // Get list of alive people (for now). May be expanded in the future to instead be
        // list of alive people in the transmitter's contact setting or household.
        // We will sample a random person from this list.
        let mut alive_people = self.query_people((Alive, true));
        if alive_people.len() > 1 {
            // In the future, we might like to sample people from the list by weights according
            // to some contact matrix.
            let mut weights = vec![1.0; alive_people.len()];
            // Get the index of the transmitter.
            let transmitter_index = alive_people
                .iter()
                .position(|&x| x == transmitter_id)
                .unwrap();
            // Remove the transmitter from the list of contacts.
            alive_people.remove(transmitter_index);
            weights.remove(transmitter_index);
            let contact_id = sample_person_from_list(self, &alive_people, &weights);
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
    use ixa::{context::Context, people::ContextPeopleExt, random::ContextRandomExt};

    #[test]
    #[should_panic(
        expected = "Cannot get a contact when there is only one person in the population."
    )]
    fn test_cant_get_contact_in_pop_of_one() {
        let mut context = Context::new();
        let transmitter = context.add_person(()).unwrap();
        let _ = context.get_contact(transmitter).unwrap();
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
