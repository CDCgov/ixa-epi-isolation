use ixa::{define_person_property_with_default, Context};
use serde::Serialize;

use crate::clinical_status_manager::{ClinicalHealthStatus, ContextClinicalExt};

#[derive(Serialize, PartialEq, Copy, Clone, Debug, Default)]
pub struct ImmunityValue {
    pub prob_immune: f64,
    pub time_last_updated: f64,
}

define_person_property_with_default!(Immunity, ImmunityValue, ImmunityValue::default());

pub struct ImmunityProgression {
    exponential_decay: f64,
}

impl ClinicalHealthStatus for ImmunityProgression {
    type Value = ImmunityValue;
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        let ImmunityValue {
            prob_immune,
            time_last_updated,
        } = *last;
        let dt = context.get_current_time() - time_last_updated;
        let prob_immune = prob_immune * (-self.exponential_decay * dt).exp();
        let next_value = ImmunityValue {
            prob_immune,
            time_last_updated: context.get_current_time(),
        };
        Some((next_value, 0.0))
    }
}

pub fn init(context: &mut Context) {
    let progression = ImmunityProgression {
        exponential_decay: 0.3,
    };
    context.register_clinical_progression(Immunity, progression);
}
