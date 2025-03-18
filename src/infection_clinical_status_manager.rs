use ixa::{Context, ContextPeopleExt, PersonProperty, PersonPropertyChangeEvent};

pub trait ClinicalHealthStatus {
    type Value;
    fn next(&self, last: &Self::Value) -> Option<(Self::Value, f64)>;
}

pub trait ContextClinicalExt {
    fn register_clinical_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl ClinicalHealthStatus<Value = T::Value> + 'static,
    );
}

impl ContextClinicalExt for Context {
    fn register_clinical_progression<T: PersonProperty + 'static>(
        &mut self,
        property: T,
        tracer: impl ClinicalHealthStatus<Value = T::Value> + 'static,
    ) {
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<T>| {
            if let Some((value, t)) = tracer.next(&event.current) {
                let current_time = context.get_current_time();
                context.add_plan(current_time + t, move |c| {
                    c.set_person_property(event.person_id, property, value);
                });
            }
        });
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
