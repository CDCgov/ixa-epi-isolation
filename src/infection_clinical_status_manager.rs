// I want two things: one that moves the patient between states, and one that tells us what's next
// for that patient

use ixa::{define_person_property_with_default, debug, Context, ContextPeopleExt, PersonProperty, PersonPropertyChangeEvent};
use serde::Serialize;

pub trait ClinicalDiseaseHistory<'a> {
    type Value;
    fn next(&self, last: &Self::Value) -> Option<(Self::Value, f64)>;
}

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum CovidStatusValue {
    Presymptomatic,
    Asymptomatic,
    Mild,
}

define_person_property_with_default!(CovidStatus, Option<CovidStatusValue>, None);

pub struct CovidHistory {
    pub states: Vec<Option<CovidStatusValue>>,
    pub time_to_event: Vec<f64>
}

impl<'a> ClinicalDiseaseHistory<'a> for CovidHistory {
    type Value = Option<CovidStatusValue>;
    fn next(&self, last: &Option<CovidStatusValue>) -> Option<(Option<CovidStatusValue>, f64)> {
        let mut iter = self.states.iter().enumerate();
        while let Some((_, status)) = iter.next() {
            if status == last {
                return iter.next().map(|(i, status)| {
                    (*status, self.time_to_event[i - 1])
                });
            }
        }
        None
    }
}

pub trait ContextClinicalExt {
    fn register_progressor<'a, T: PersonProperty + std::fmt::Debug + 'static>(&mut self, property: T, progressor: impl ClinicalDiseaseHistory<'static, Value = T::Value> + 'static);
}

impl ContextClinicalExt for Context {
    fn register_progressor<'a, T: PersonProperty + std::fmt::Debug + 'static>(&mut self, property: T, progressor: impl ClinicalDiseaseHistory<'static, Value = T::Value> + 'static) {
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<T>| {
            debug!("Handling person property change event.");
            if let Some((value, t)) = progressor.next(&event.current) {
                debug!("Setting person property {:?} to {:?} in {:?} days.", property, value, t);
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

    use ixa::{set_log_level, Context, ContextPeopleExt, LevelFilter};
    use statrs::assert_almost_eq;

    use super::{ClinicalDiseaseHistory, ContextClinicalExt, CovidHistory, CovidStatus, CovidStatusValue};

    #[test]
    fn test_covid_history() {
        let history = CovidHistory {
            states: vec![Some(CovidStatusValue::Presymptomatic), Some(CovidStatusValue::Asymptomatic), Some(CovidStatusValue::Mild)],
            time_to_event: vec![1.0, 2.0]
        };
        assert_eq!(history.next(&Some(CovidStatusValue::Presymptomatic)), Some((Some(CovidStatusValue::Asymptomatic), 1.0)));
        assert_eq!(history.next(&Some(CovidStatusValue::Asymptomatic)), Some((Some(CovidStatusValue::Mild), 2.0)));
        assert_eq!(history.next(&Some(CovidStatusValue::Mild)), None);
    }

    #[test]
    fn test_covid_progressor() {
        let history = CovidHistory {
            states: vec![Some(CovidStatusValue::Presymptomatic), Some(CovidStatusValue::Asymptomatic), Some(CovidStatusValue::Mild)],
            time_to_event: vec![1.0, 2.0]
        };
        let mut context = Context::new();
        set_log_level(LevelFilter::Debug);
        context.register_progressor(CovidStatus, history);
        let person_id = context.add_person(()).unwrap();
        context.set_person_property(person_id, CovidStatus, Some(CovidStatusValue::Presymptomatic));
        context.execute();
        assert_almost_eq!(context.get_current_time(), 3.0, 0.0);
        assert_eq!(context.get_person_property(person_id, CovidStatus), Some(CovidStatusValue::Mild));
    }
}