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
    fn register_parameter_assignment<T, S, I>(
        &mut self,
        parameter: T,
        setter: S,
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
    fn register_parameter_assignment<T, S, I>(
        &mut self,
        _parameter: T,
        setter: S,
    ) -> Result<(), IxaError>
    where
        T: NaturalHistoryParameter + 'static,
        S: Fn(&Context, PersonId) -> I + 'static,
        I: NaturalHistoryId + 'static,
    {
        let container = self.get_data_container_mut(NaturalHistoryParameters);
        match container.parameter_assignments.entry(TypeId::of::<T>()) {
            Entry::Vacant(_) => {
                container.parameter_assignments.insert(
                    TypeId::of::<T>(),
                    Box::new(move |ctx, person_id| Box::new(setter(ctx, person_id))),
                );
            }

            Entry::Occupied(_) => {
                return Err(IxaError::IxaError(
                    "Parameter assignment already registered.".to_string(),
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
        // Is there an assignment for this parameter?
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
        let id = self.sample_range(NaturalHistoryParametersRng, 0..parameter.library_size());
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
    fn init() {
        let mut context = Context::new();
        context
            .register_parameter_assignment(ViralLoad, |_ctx, _person_id| 0)
            .unwrap();

        context
            .register_parameter_assignment(ViralLoad, |ctx, person_id| {
                ctx.get_person_property(person_id, Age) as usize
            })
            .unwrap();
    }
}
