use std::{
    any::TypeId,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
};

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError, PersonId};

define_rng!(NaturalHistoryParameterRng);

/// Specifies behavior for a library of natural history parameters
pub trait NaturalHistoryParameterLibrary {
    /// Returns the size of a library of natural history parameters. Used to return a random index
    /// in the range of the library size when querying an id for a natural history parameter that
    /// has no assignment function previously registered.
    fn library_size(&self, context: &Context) -> usize;
}

#[derive(Default)]
#[allow(clippy::type_complexity)]
struct NaturalHistoryParameterContainer {
    parameter_id_assigners: HashMap<TypeId, Box<dyn Fn(&Context, PersonId) -> usize>>,
    ids: RefCell<HashMap<TypeId, HashMap<PersonId, usize>>>,
}

define_data_plugin!(
    NaturalHistoryParameters,
    NaturalHistoryParameterContainer,
    NaturalHistoryParameterContainer::default()
);

/// Provides methods for specifying relationships between libraries of natural history parameters.
pub trait ContextNaturalHistoryParameterExt {
    /// Register the function used to assign a particular index from a library of natural history
    /// parameters `T` to a person. The function is evaluated when the id is requested, so it can
    /// depend on other parameters and takes `&Context` and `PersonId` as arguments. Values from
    /// the registered function are returned when `context.get_parameter_id` is called.
    /// # Errors
    /// - If an assignment function for the parameter has already been registered
    /// - If an id for the parameter has already been queried for a person (i.e., defaulting to
    ///   random assignment)
    fn register_parameter_id_assigner<T, S>(
        &mut self,
        parameter: T,
        assignment_fn: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameterLibrary + 'static,
        S: Fn(&Context, PersonId) -> usize + 'static;

    /// Get the id for a natural history parameter for a person. If an assignment function is
    /// registered, returns the id from evaluating that function, and if no assignment function is
    /// registered, returns a random id between 0 (inclusive) and the parameter's library size
    /// (exclusive).
    /// Stores the assigned id so that calling this function again with the same parameter and
    /// person will return the same id.
    /// Does not check whether the id returned from an assignment function is in the range of the
    /// library size.
    fn get_parameter_id<T>(&self, parameter: T, person_id: PersonId) -> usize
    where
        T: NaturalHistoryParameterLibrary + 'static;
}

impl ContextNaturalHistoryParameterExt for Context {
    fn register_parameter_id_assigner<T, S>(
        &mut self,
        _parameter: T,
        assignment_fn: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameterLibrary + 'static,
        S: Fn(&Context, PersonId) -> usize + 'static,
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
                entry.insert(Box::new(assignment_fn));
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
        T: NaturalHistoryParameterLibrary + 'static,
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
            let id = assigner(self, person_id);
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
    use std::{any::TypeId, collections::HashMap, path::PathBuf};

    use ixa::{
        context::Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError,
    };

    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, Params, ProgressionLibraryType, RateFnType},
        rate_fns::{load_rate_fns, RateFn},
        symptom_progression::Symptoms,
    };

    use super::{
        ContextNaturalHistoryParameterExt, NaturalHistoryParameterLibrary, NaturalHistoryParameters,
    };

    struct ViralLoad;

    impl NaturalHistoryParameterLibrary for ViralLoad {
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
        assert_eq!(assigner(&context, person), 0);
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

    impl NaturalHistoryParameterLibrary for AntigenPositivity {
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

    impl NaturalHistoryParameterLibrary for CulturePositivity {
        fn library_size(&self, _context: &Context) -> usize {
            10000
        }
    }

    struct TestingPatterns;

    impl NaturalHistoryParameterLibrary for TestingPatterns {
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

    #[test]
    fn test_real_file_ids() {
        let mut context = init_context();

        let parameters = Params {
            initial_infections: 0,
            initial_recovered: 0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_empirical_rate_fns.csv"),
                scale: 1.0,
            },
            symptom_progression_library: Some(ProgressionLibraryType::EmpiricalFromFile {
                file: PathBuf::from("./input/library_symptom_parameters.csv"),
            }),
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };

        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        // Add a person
        let person = context.add_person(()).unwrap();
        // Infect the person -- we have to do this above the call to the symptom progression
        // init because otherwise it panics that the people module has not been initialized?
        context.infect_person(person, None, None, None);

        // Initialize symptoms -- reads in the symptom progression library
        load_rate_fns(&mut context).unwrap();
        crate::symptom_progression::init(&mut context).unwrap();
        // Read in the rate function library

        // See if the person's symptom id is the same as their rate function id
        assert_eq!(
            context.get_parameter_id(Symptoms, person),
            context.get_parameter_id(RateFn, person)
        );
    }
}
