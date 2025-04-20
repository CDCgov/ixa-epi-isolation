use std::{
    any::TypeId,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
};

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError, PersonId};

define_rng!(NaturalHistoryParameterRng);

pub trait NaturalHistoryParameter {
    fn library_size(&self, context: &Context) -> usize;
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
struct NaturalHistoryParameterContainer {
    parameter_id_assigners: HashMap<TypeId, Box<IdAssigner<Box<dyn NaturalHistoryId>>>>,
    ids: RefCell<HashMap<TypeId, HashMap<PersonId, usize>>>,
}

define_data_plugin!(
    NaturalHistoryParameters,
    NaturalHistoryParameterContainer,
    NaturalHistoryParameterContainer::default()
);

pub trait ContextNaturalHistoryParameterExt {
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

impl ContextNaturalHistoryParameterExt for Context {
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
        if container.ids.borrow().contains_key(&TypeId::of::<T>()) {
            return Err(IxaError::IxaError(
                "An id for this parameter has been previously queried, so a new assignment function cannot be specified.
                If this is desired behavior, register an assignment function that changes from random to the specified
                behavior at the time at which this assignment is registered.".to_string(),
            ));
        }

        // Only register this assignment function if this parameter does not have a previously
        // registered assignment function.
        match container.parameter_id_assigners.entry(TypeId::of::<T>()) {
            Entry::Vacant(entry) => {
                entry.insert(Box::new(move |ctx, person_id| {
                    Box::new(assignment_fn(ctx, person_id))
                }));
                Ok(())
            }

            Entry::Occupied(_) => Err(IxaError::IxaError(
                "An assignment function for this parameter has already been registered."
                    .to_string(),
            )),
        }
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
        if let Some(assigner) = container.parameter_id_assigners.get(&TypeId::of::<T>()) {
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
        let library_size = parameter.library_size(self);
        let id = self.sample_range(NaturalHistoryParameterRng, 0..library_size);
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
    use std::any::TypeId;

    use ixa::{context::Context, ContextPeopleExt, ContextRandomExt, IxaError};

    use super::{
        ContextNaturalHistoryParameterExt, NaturalHistoryParameter, NaturalHistoryParameters,
    };

    struct ViralLoad;

    impl NaturalHistoryParameter for ViralLoad {
        fn library_size(&self, _context: &Context) -> usize {
            1
        }
    }

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
    }

    #[test]
    fn test_register_vanilla_parameter_id_assignment() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        context
            .register_parameter_id_assignment(ViralLoad, |_, _| 0)
            .unwrap();
        let container = context
            .get_data_container(NaturalHistoryParameters)
            .unwrap();
        assert_eq!(container.parameter_id_assigners.len(), 1);
        let assigner = container
            .parameter_id_assigners
            .get(&TypeId::of::<ViralLoad>())
            .unwrap();
        assert_eq!(assigner(&context, person).id(), 0);
    }

    #[test]
    fn test_error_register_two_assignments_same_parameter() {
        let mut context = init_context();
        context
            .register_parameter_id_assignment(ViralLoad, |_, _| 0)
            .unwrap();
        let result = context.register_parameter_id_assignment(ViralLoad, |_, _| 1);
        let e = result.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "An assignment function for this parameter has already been registered.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that an assignment function for this parameter has already been registered.. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, function registration passed with no errors."),
        }
    }

    struct AntigenPositivity;

    impl NaturalHistoryParameter for AntigenPositivity {
        fn library_size(&self, _context: &Context) -> usize {
            1
        }
    }

    #[test]
    fn test_error_register_assignment_after_querying() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        // We have to register something to make sure the data container exists.
        context
            .register_parameter_id_assignment(ViralLoad, |_, _| 0)
            .unwrap();
        // This section also tests
        let result = context.get_parameter_id(AntigenPositivity, person);
        assert_eq!(result, 0);
        let result = context.register_parameter_id_assignment(AntigenPositivity, |_, _| 1);
        let e = result.err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "An id for this parameter has been previously queried, so a new assignment function cannot be specified.
                If this is desired behavior, register an assignment function that changes from random to the specified
                behavior at the time at which this assignment is registered.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that an id for this parameter has been previously queried so an assignment function cannot be registered. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, function registration passed with no errors."),
        }
    }

    #[test]
    fn test_register_multiple_parameter_id_assignments() {}

    #[test]
    fn test_get_parameter_id_registered_assignment() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        context
            .register_parameter_id_assignment(ViralLoad, |_, _| 0)
            .unwrap();
        let id = context.get_parameter_id(ViralLoad, person);
        assert_eq!(id, 0);
    }

    #[test]
    fn test_get_parameter_id_already_set() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        let container = context.get_data_container_mut(NaturalHistoryParameters);
        container.ids.borrow_mut().insert(
            TypeId::of::<ViralLoad>(),
            vec![(person, 0)].into_iter().collect(),
        );
        let id = context.get_parameter_id(ViralLoad, person);
        assert_eq!(id, 0);
    }

    struct CulturePositivity;

    impl NaturalHistoryParameter for CulturePositivity {
        fn library_size(&self, _context: &Context) -> usize {
            10000
        }
    }

    struct TestingPatterns;

    impl NaturalHistoryParameter for TestingPatterns {
        fn library_size(&self, _context: &Context) -> usize {
            10000
        }
    }

    #[test]
    fn test_get_parameter_id_assignment_has_dependencies() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        context
            .register_parameter_id_assignment(CulturePositivity, |context, person_id| {
                context.get_parameter_id(TestingPatterns, person_id)
            })
            .unwrap();
        let culture_id = context.get_parameter_id(CulturePositivity, person);
        let testing_id = context.get_parameter_id(TestingPatterns, person);
        assert_eq!(culture_id, testing_id);
    }
}
