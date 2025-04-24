use std::{
    any::TypeId,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
};

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError, PersonId};

define_rng!(NaturalHistoryParameterRng);

/// Specifies behavior for a library of natural history parameters
pub trait NaturalHistoryParameter {
    /// Returns the size of a library of natural history parameters.
    fn library_size(&self, context: &Context) -> usize;
}

/// Specifies behavior for obtaining a natural history id from a type.
pub trait NaturalHistoryId {
    /// Returns the index of a natural history parameter in a library of parameters.
    fn id(&self) -> usize;
}

// By default, natural history indeces will be usize. This is a convenience implementation for
// common user code that returns a usize id as the natural history id.
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

/// Provides methods for specifying relationships between libraries of natural history parameters.
pub trait ContextNaturalHistoryParameterExt {
    /// Register an assignment function for the id of a natural history parameter. The assignment
    /// function takes `Context` and `PersonId` as arguments and returns a type `I` that implements
    /// `NaturalHistoryId`. The id refers to the index of an item in a library of natural history
    /// parameters.
    /// # Errors
    /// - If an assignment function for the parameter has already been registered
    /// - If an id for the parameter has already been queried for a person (i.e., defaulting to
    ///   random assignment)
    fn register_parameter_id_assigner<T, S, I>(
        &mut self,
        parameter: T,
        assignment_fn: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameter + 'static,
        S: Fn(&Context, PersonId) -> I + 'static,
        I: NaturalHistoryId + 'static;

    /// Get the id for a parameter for a person according to the registered assignment function. If
    /// no assignment function is registered, a random id will be assigned between 0 (inclusive) and
    /// the parameter's library size (exclusive).
    /// Stores the assigned id so that calling this function again with the same parameter and
    /// person will return the same id.
    /// Does not check whether the id returned from an assignment function is in the range of the
    /// library size.
    fn get_parameter_id<T>(&self, parameter: T, person_id: PersonId) -> usize
    where
        T: NaturalHistoryParameter + 'static;
}

impl ContextNaturalHistoryParameterExt for Context {
    fn register_parameter_id_assigner<T, S, I>(
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
        let container = self
        .get_data_container(NaturalHistoryParameters)
        .expect("Natural history parameter ids cannot be queried unless at least one parameter assignment is registered.");
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
        if let Some(assigner) = container
            .parameter_id_assigners
            .get(&TypeId::of::<T>())
            .map(|x| x.as_ref())
        {
            // Get the id from the assignment function
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
            .register_parameter_id_assigner(ViralLoad, |_, _| 0)
            .unwrap();
        let container = context
            .get_data_container(NaturalHistoryParameters)
            .unwrap();
        assert_eq!(container.parameter_id_assigners.len(), 1);
        let assigner = container
            .parameter_id_assigners
            .get(&TypeId::of::<ViralLoad>())
            .unwrap()
            .as_ref();
        assert_eq!(assigner(&context, person).id(), 0);
    }

    #[test]
    fn test_error_register_two_assignments_same_parameter() {
        let mut context = init_context();
        context
            .register_parameter_id_assigner(ViralLoad, |_, _| 0)
            .unwrap();
        let result = context.register_parameter_id_assigner(ViralLoad, |_, _| 1);
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
            .register_parameter_id_assigner(ViralLoad, |_, _| 0)
            .unwrap();
        // This section also tests that we default to random assignment in library range for
        // parameters not previously seen if some registration has occured previously.
        let result = context.get_parameter_id(AntigenPositivity, person);
        assert_eq!(result, 0);
        let result = context.register_parameter_id_assigner(AntigenPositivity, |_, _| 1);
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
    #[should_panic(
        expected = "Natural history parameter ids cannot be queried unless at least one parameter assignment is registered."
    )]
    fn test_error_query_property_id_before_registration() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        context.get_parameter_id(ViralLoad, person);
    }

    #[test]
    fn test_get_parameter_id_registered_assignment() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();
        context
            .register_parameter_id_assigner(ViralLoad, |_, _| 0)
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
            .register_parameter_id_assigner(CulturePositivity, |context, person_id| {
                context.get_parameter_id(TestingPatterns, person_id)
            })
            .unwrap();
        let culture_id = context.get_parameter_id(CulturePositivity, person);
        let testing_id = context.get_parameter_id(TestingPatterns, person);
        assert_eq!(culture_id, testing_id);
    }
}
