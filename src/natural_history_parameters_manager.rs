use std::{
    any::TypeId,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError, PersonId};

define_rng!(NaturalHistoryParametersRng);

pub trait NaturalHistoryParameter {
    fn library_size(&self) -> usize;
}

pub trait NaturalHistoryId {
    fn id(&self) -> usize;
}

impl NaturalHistoryId for usize {
    fn id(&self) -> usize {
        *self
    }
}

type IdAssigner<I> = dyn Fn(&Context, PersonId) -> I;

#[derive(Default)]
struct NaturalHistoryParametersContainer {
    parameter_assignments: HashMap<TypeId, Box<IdAssigner<Box<dyn NaturalHistoryId>>>>,
    ids: Rc<RefCell<HashMap<TypeId, HashMap<PersonId, usize>>>>,
}

define_data_plugin!(
    NaturalHistoryParameters,
    NaturalHistoryParametersContainer,
    NaturalHistoryParametersContainer::default()
);

pub trait ContextNaturalHistoryParametersExt {
    fn register_parameter_id_assignment<T, S, I>(
        &mut self,
        parameter: T,
        assignment_fn: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameter + 'static,
        S: Fn(&Context, PersonId) -> I + 'static,
        I: NaturalHistoryId + 'static;
    fn get_parameter_id<T>(&self, parameter: T, person_id: PersonId) -> usize
    where
        T: NaturalHistoryParameter + 'static;
}

impl ContextNaturalHistoryParametersExt for Context {
    fn register_parameter_id_assignment<T, S, I>(
        &mut self,
        _parameter: T,
        assignment_fn: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameter + 'static,
        S: Fn(&Context, PersonId) -> I + 'static,
        I: NaturalHistoryId + 'static,
    {
        let container = self.get_data_container_mut(NaturalHistoryParameters);
        
        // We shouldn't be registering assignment functions for parameters where we've already been
        // asked to provide an id and defaulted to random assignment. In this case, the assignment
        // function does not apply to all ids, so the id assignment is ambiguous.
        if container
            .ids
            .borrow()
            .contains_key(&TypeId::of::<T>())
        {
            return Err(IxaError::IxaError(
                "An id for this parameter has been previously queried, so a new assignment function cannot be specified.
                If this is desired behavior, register an assignment function that changes from random to the specified
                behavior at the time at which this assignment is registered.".to_string(),
            ));
        }
        
        // Only register this assignment function if this parameter does not have a previously
        // registered assignment function.
        match container.parameter_assignments.entry(TypeId::of::<T>()) {
            Entry::Vacant(_) => {
                container.parameter_assignments.insert(
                    TypeId::of::<T>(),
                    Box::new(move |ctx, person_id| Box::new(assignment_fn(ctx, person_id))),
                );
            }

            Entry::Occupied(_) => {
                return Err(IxaError::IxaError(
                    "An assignment function for this parameter has already been registered.".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn get_parameter_id<T>(&self, parameter: T, person_id: PersonId) -> usize
    where
        T: NaturalHistoryParameter + 'static,
    {
        let container = self.get_data_container(NaturalHistoryParameters).unwrap();
        // Is there already an id for this person for this parameter?
        if let Some(potential_id) = container
            .ids
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|ids| ids.get(&person_id))
        {
            return *potential_id;
        }

        // Is there an assignment function for this parameter?
        if let Some(assigner) = container.parameter_assignments.get(&TypeId::of::<T>()) {
            // Get the id from the assignment
            let id = assigner(self, person_id).id();
            // Store the id for this person
            container
                .ids
                .borrow_mut()
                .entry(TypeId::of::<T>())
                .or_default()
                .insert(person_id, id);
            return id;
        }

        // Else make a default random assignment and store it
        let library_size = parameter.library_size();
        let id = self.sample_range(NaturalHistoryParametersRng, 0..library_size);
        container
            .ids
            .borrow_mut()
            .entry(TypeId::of::<T>())
            .or_default()
            .insert(person_id, id);
        id
    }
}

#[cfg(test)]
mod test {
    use ixa::{context::Context, ContextPeopleExt};

    use crate::population_loader::Age;

    use super::{ContextNaturalHistoryParametersExt, NaturalHistoryParameter};

    struct ViralLoad;

    impl NaturalHistoryParameter for ViralLoad {
        fn library_size(&self) -> usize {
            1
        }
    }

    #[test]
    fn test_register_vanilla_parameter_id_assignment() {

    }

    #[test]
    fn test_error_register_two_assignments_same_parameter() {

    }

    #[test]
    fn test_error_register_assignment_after_querying() {

    }

    #[test]
    fn test_register_multiple_parameter_id_assignments() {

    }

    #[test]
    fn test_get_parameter_id_registered_assignment() {

    }

    #[test]
    fn test_get_parameter_id_already_set() {

    }

    #[test]
    fn test_get_parameter_id_random_assignment() {

    }

    #[test]
    fn test_get_parameter_id_assignment_has_dependencies() {

    }
}
