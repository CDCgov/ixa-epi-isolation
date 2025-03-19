use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use ixa::{
    define_data_plugin, Context, ContextPeopleExt, PersonProperty, PersonPropertyChangeEvent,
};

pub trait ClinicalHealthStatus {
    fn next(&self, last: Box<dyn Any>) -> Option<(Box<dyn Any>, f64)>;
}

#[derive(Default)]
struct ClinicalProgressionContainer {
    progressions: HashMap<TypeId, Vec<Box<dyn ClinicalHealthStatus>>>,
}

define_data_plugin!(
    ClinicalProgression,
    ClinicalProgressionContainer,
    ClinicalProgressionContainer::default()
);

pub trait ContextClinicalExt {
    fn register_clinical_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl ClinicalHealthStatus + 'static,
    );
}

impl ContextClinicalExt for Context {
    fn register_clinical_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl ClinicalHealthStatus + 'static,
    ) {
        // Add tracer to data container
        // Subscribe to event if hashmap has not yet considered this person property
        // Make sure to get the right tracer out for the person
        let container = self.get_data_container_mut(ClinicalProgression);
        let progressions = container.progressions.entry(TypeId::of::<T>()).or_default();
        progressions.push(Box::new(tracer));
        if progressions.len() == 1 {
            self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<T>| {
                let container = context.get_data_container(ClinicalProgression).unwrap();
                let progressions = container.progressions.get(&TypeId::of::<T>()).unwrap();
                // Just for argument's sake, let's only ever take the first tracer.
                let tcr = &progressions[0];
                if let Some((next_value, time_to_next)) = tcr.next(Box::new(event.current)) {
                    let current_time = context.get_current_time();
                    context.add_plan(current_time + time_to_next, move |ctx| {
                        ctx.set_person_property(
                            event.person_id,
                            property,
                            *next_value.downcast_ref::<T::Value>().unwrap(),
                        );
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod test {

    use ixa::{Context, ContextPeopleExt};
    use statrs::assert_almost_eq;

    use crate::symptom_progression::{
        DiseaseSeverity, DiseaseSeverityProgression, DiseaseSeverityValue,
    };

    use super::ContextClinicalExt;

    #[test]
    fn test_register_clinical_progression_automates_moves() {
        let progression = DiseaseSeverityProgression::new(
            vec![
                Some(DiseaseSeverityValue::Presymptomatic),
                Some(DiseaseSeverityValue::Asymptomatic),
                Some(DiseaseSeverityValue::Mild),
            ],
            vec![1.0, 2.0],
        );
        let mut context = Context::new();
        context.register_clinical_progression(DiseaseSeverity, progression);
        let person_id = context.add_person(()).unwrap();
        context.set_person_property(
            person_id,
            DiseaseSeverity,
            Some(DiseaseSeverityValue::Presymptomatic),
        );
        context.execute();
        assert_almost_eq!(context.get_current_time(), 3.0, 0.0);
        assert_eq!(
            context.get_person_property(person_id, DiseaseSeverity),
            Some(DiseaseSeverityValue::Mild)
        );
    }
}
