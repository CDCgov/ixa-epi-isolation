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

pub struct EmpiricalProgression<T: PartialEq + Copy> {
    states: Vec<T>,
    time_to_next: Vec<f64>,
}

impl<T: PartialEq + Copy> EmpiricalProgression<T> {
    pub fn new(states: Vec<T>, time_to_next: Vec<f64>) -> EmpiricalProgression<T> {
        EmpiricalProgression {
            states,
            time_to_next,
        }
    }
}

impl<T: PartialEq + Copy> ClinicalHealthStatus for EmpiricalProgression<T> {
    type Value = T;
    fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        let mut iter = self.states.iter().enumerate();
        while let Some((_, status)) = iter.next() {
            if status == last {
                return iter
                    .next()
                    .map(|(i, next)| (*next, self.time_to_next[i - 1]));
            }
        }
        None
    }
}

pub fn init(context: &mut Context) {
    // Todo(kzs9): We will read these progressions from a file from our isolation guidance modeling
    let progression1 = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Presymptomatic,
            DiseaseSeverityValue::Mild,
            DiseaseSeverityValue::Moderate,
            DiseaseSeverityValue::Severe,
            DiseaseSeverityValue::Healthy,
        ],
        vec![1.0, 2.0, 1.0, 4.0],
    );
    context.register_clinical_progression(DiseaseSeverity, progression1);

    let progression2 = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Asymptomatic,
            DiseaseSeverityValue::Healthy,
        ],
        vec![2.0],
    );
    context.register_clinical_progression(DiseaseSeverity, progression2);

    let progression3 = EmpiricalProgression::new(
        vec![
            DiseaseSeverityValue::Presymptomatic,
            DiseaseSeverityValue::Mild,
            DiseaseSeverityValue::Healthy,
        ],
        vec![2.0, 3.0],
    );
    context.register_clinical_progression(DiseaseSeverity, progression3);
}

#[cfg(test)]
mod test {
    use super::{DiseaseSeverityValue, EmpiricalProgression};
    use crate::clinical_status_manager::ClinicalHealthStatus;

    use ixa::Context;
    use statrs::assert_almost_eq;

    #[test]
    fn test_disease_progression() {
        let context = Context::new();
        let progression = EmpiricalProgression::new(
            vec![
                DiseaseSeverityValue::Presymptomatic,
                DiseaseSeverityValue::Asymptomatic,
                DiseaseSeverityValue::Mild,
            ],
            vec![1.0, 2.0],
        );
        let initial_state = DiseaseSeverityValue::Presymptomatic;
        let (next_state, time) = progression.next(&context, &initial_state).unwrap();
        assert_eq!(next_state, DiseaseSeverityValue::Asymptomatic);
        assert_almost_eq!(time, 1.0, 0.0);

        let initial_state = DiseaseSeverityValue::Asymptomatic;
        let (next_state, time) = progression.next(&context, &initial_state).unwrap();
        assert_eq!(next_state, DiseaseSeverityValue::Mild);
        assert_almost_eq!(time, 2.0, 0.0);

        let initial_state = DiseaseSeverityValue::Mild;
        let next_state = progression.next(&context, &initial_state);
        assert!(next_state.is_none());
    }
}
