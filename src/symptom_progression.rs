use std::any::Any;

use ixa::{define_person_property_with_default, Context};
use serde::Serialize;

use crate::clinical_status_manager::{ClinicalHealthStatus, ContextClinicalExt};

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum DiseaseSeverityValue {
    Healthy,
    Presymptomatic,
    Asymptomatic,
    Mild,
    Moderate,
    Severe,
}

define_person_property_with_default!(
    DiseaseSeverity,
    DiseaseSeverityValue,
    DiseaseSeverityValue::Healthy
);

pub struct DiseaseSeverityProgression {
    states: Vec<DiseaseSeverityValue>,
    time_to_next: Vec<f64>,
}

impl DiseaseSeverityProgression {
    pub fn new(
        states: Vec<DiseaseSeverityValue>,
        time_to_next: Vec<f64>,
    ) -> DiseaseSeverityProgression {
        DiseaseSeverityProgression {
            states,
            time_to_next,
        }
    }
}

impl ClinicalHealthStatus for DiseaseSeverityProgression {
    fn next(&self, last: Box<dyn Any>) -> Option<(Box<dyn Any>, f64)> {
        let mut iter = self.states.iter().enumerate();
        let last_value = last.downcast_ref::<DiseaseSeverityValue>().unwrap();
        while let Some((_, status)) = iter.next() {
            if status == last_value {
                return iter
                    .next()
                    .map(|(i, next)| (Box::new(*next) as Box<dyn Any>, self.time_to_next[i - 1]));
            }
        }
        None
    }
}

pub fn init(context: &mut Context) {
    let progression = DiseaseSeverityProgression::new(
        vec![
            DiseaseSeverityValue::Presymptomatic,
            DiseaseSeverityValue::Asymptomatic,
            DiseaseSeverityValue::Mild,
            DiseaseSeverityValue::Moderate,
            DiseaseSeverityValue::Severe,
        ],
        vec![1.0, 2.0, 1.0, 1.0],
    );
    context.register_clinical_progression(DiseaseSeverity, progression);
}

#[cfg(test)]
mod test {
    use super::{DiseaseSeverityProgression, DiseaseSeverityValue};
    use crate::clinical_status_manager::ClinicalHealthStatus;
    use std::any::Any;

    use statrs::assert_almost_eq;

    #[test]
    fn test_disease_progression() {
        let progression = DiseaseSeverityProgression::new(
            vec![
                DiseaseSeverityValue::Presymptomatic,
                DiseaseSeverityValue::Asymptomatic,
                DiseaseSeverityValue::Mild,
            ],
            vec![1.0, 2.0],
        );
        let initial_state = Box::new(DiseaseSeverityValue::Presymptomatic) as Box<dyn Any>;
        let (next_state, time) = progression.next(initial_state).unwrap();
        assert_eq!(
            *next_state.downcast_ref::<DiseaseSeverityValue>().unwrap(),
            DiseaseSeverityValue::Asymptomatic
        );
        assert_almost_eq!(time, 1.0, 0.0);

        let initial_state = Box::new(DiseaseSeverityValue::Asymptomatic) as Box<dyn Any>;
        let (next_state, time) = progression.next(initial_state).unwrap();
        assert_eq!(
            *next_state.downcast_ref::<DiseaseSeverityValue>().unwrap(),
            DiseaseSeverityValue::Mild
        );
        assert_almost_eq!(time, 2.0, 0.0);

        let initial_state = Box::new(DiseaseSeverityValue::Mild) as Box<dyn Any>;
        let next_state = progression.next(initial_state);
        assert!(next_state.is_none());
    }
}
