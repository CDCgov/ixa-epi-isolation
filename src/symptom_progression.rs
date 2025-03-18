use ixa::{define_person_property_with_default, Context};
use serde::Serialize;

use crate::infection_clinical_status_manager::{ClinicalHealthStatus, ContextClinicalExt};

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum DiseaseSeverityValue {
    Presymptomatic,
    Asymptomatic,
    Mild,
    Moderate,
    Severe,
}

define_person_property_with_default!(DiseaseSeverity, Option<DiseaseSeverityValue>, None);

pub struct DiseaseSeverityProgression {
    states: Vec<Option<DiseaseSeverityValue>>,
    time_to_next: Vec<f64>,
}

impl DiseaseSeverityProgression {
    pub fn new(
        states: Vec<Option<DiseaseSeverityValue>>,
        time_to_next: Vec<f64>,
    ) -> DiseaseSeverityProgression {
        DiseaseSeverityProgression {
            states,
            time_to_next,
        }
    }
}

impl ClinicalHealthStatus for DiseaseSeverityProgression {
    type Value = Option<DiseaseSeverityValue>;
    fn next(
        &self,
        last: &Option<DiseaseSeverityValue>,
    ) -> Option<(Option<DiseaseSeverityValue>, f64)> {
        let mut iter = self.states.iter().enumerate();
        while let Some((_, status)) = iter.next() {
            if status == last {
                return iter
                    .next()
                    .map(|(i, status)| (*status, self.time_to_next[i - 1]));
            }
        }
        None
    }
}

pub fn init(context: &mut Context) {
    let progression = DiseaseSeverityProgression::new(
        vec![
            Some(DiseaseSeverityValue::Presymptomatic),
            Some(DiseaseSeverityValue::Asymptomatic),
            Some(DiseaseSeverityValue::Mild),
            Some(DiseaseSeverityValue::Moderate),
            Some(DiseaseSeverityValue::Severe),
        ],
        vec![1.0, 2.0, 1.0, 1.0],
    );
    context.register_clinical_progression(DiseaseSeverity, progression);
}

#[cfg(test)]
mod test {
    use super::{DiseaseSeverityProgression, DiseaseSeverityValue};
    use crate::infection_clinical_status_manager::ClinicalHealthStatus;

    #[test]
    fn test_disease_progression() {
        let progression = DiseaseSeverityProgression::new(
            vec![
                Some(DiseaseSeverityValue::Presymptomatic),
                Some(DiseaseSeverityValue::Asymptomatic),
                Some(DiseaseSeverityValue::Mild),
            ],
            vec![1.0, 2.0],
        );
        assert_eq!(
            progression.next(&Some(DiseaseSeverityValue::Presymptomatic)),
            Some((Some(DiseaseSeverityValue::Asymptomatic), 1.0))
        );
        assert_eq!(
            progression.next(&Some(DiseaseSeverityValue::Asymptomatic)),
            Some((Some(DiseaseSeverityValue::Mild), 2.0))
        );
        assert_eq!(progression.next(&Some(DiseaseSeverityValue::Mild)), None);
    }
}
