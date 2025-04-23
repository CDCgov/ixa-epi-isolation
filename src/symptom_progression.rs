use ixa::{
    define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt,
    IxaError, PersonId, PersonPropertyChangeEvent,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use statrs::distribution::{ContinuousCDF, Weibull};

use crate::{
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    parameters::ContextParametersExt,
    property_progression_manager::{
        load_progression_library, ContextPropertyProgressionExt, Progression,
    },
};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum SymptomValue {
    Presymptomatic,
    Category1,
    Category2,
    Category3,
    Category4,
}

define_person_property_with_default!(Symptoms, Option<SymptomValue>, None);

/// Stores information about a symptom progression (presymptomatic -> category{1..=4} -> None).
/// Includes details about the incubation period and time to symptom improvement distributions.
pub struct SymptomData {
    category: SymptomValue,
    incubation_period: LowerTruncatedLogNormal,
    time_to_symptom_improvement: UpperTruncatedDiscreteWeibull,
}

#[derive(Copy, Clone)]
struct LowerTruncatedLogNormal {
    mean: f64,
    std_dev: f64,
    lower_bound: f64,
}

impl LowerTruncatedLogNormal {
    fn new(mean: f64, std_dev: f64, lower_bound: f64) -> Result<Self, IxaError> {
        if mean <= 0.0 {
            return Err(IxaError::IxaError(
                "Mean of log-normal distribution must be positive.".to_string(),
            ));
        }
        if std_dev <= 0.0 {
            return Err(IxaError::IxaError(
                "Standard deviation of log-normal distribution must be positive.".to_string(),
            ));
        }
        Ok(Self {
            mean,
            std_dev,
            lower_bound,
        })
    }
}

#[derive(Copy, Clone)]
struct UpperTruncatedDiscreteWeibull {
    shape: f64,
    scale: f64,
    upper_bound: f64,
}

impl UpperTruncatedDiscreteWeibull {
    fn new(shape: f64, scale: f64, upper_bound: f64) -> Result<Self, IxaError> {
        if shape <= 0.0 {
            return Err(IxaError::IxaError(
                "Shape of Weibull distribution must be positive.".to_string(),
            ));
        }
        if scale <= 0.0 {
            return Err(IxaError::IxaError(
                "Scale of Weibull distribution must be positive.".to_string(),
            ));
        }
        Ok(Self {
            shape,
            scale,
            upper_bound,
        })
    }
}

impl SymptomData {
    pub fn register(
        context: &mut Context,
        parameter_names: &[String],
        parameters: &[f64],
    ) -> Result<(), IxaError> {
        assert_eq!(
            parameter_names.len(),
            parameters.len(),
            "Parameter names and parameters must be of the same length."
        );
        // The first parameter is the symptom category name, the next three are the
        // parameters for the incubation period distribution, and the final three are
        // the parameters for the Weibull distribution.
        if parameter_names.len() != 7 {
            return Err(IxaError::IxaError(format!(
                "Parameters should be of length 7, but got {}",
                parameter_names.len()
            )));
        }

        // Get out the symptom category name
        let category: SymptomValue = from_str(&parameter_names[0])?;
        if !parameters[0].is_nan() {
            return Err(IxaError::IxaError(format!(
                            "Parameter associated with specifying the symptom category name should be NaN, but got {}",
                            parameters[0]
                        )));
        }

        // Get out the incubation period parameters
        let incubation_period =
            LowerTruncatedLogNormal::new(parameters[1], parameters[2], parameters[3])?;

        // Get out the Weibull parameters
        let time_to_symptom_improvement =
            UpperTruncatedDiscreteWeibull::new(parameters[4], parameters[5], parameters[6])?;
        let progression = SymptomData {
            category,
            incubation_period,
            time_to_symptom_improvement,
        };
        context.register_property_progression(Symptoms, progression);
        Ok(())
    }
}

impl Progression<Symptoms> for SymptomData {
    fn next(
        &self,
        context: &Context,
        _person_id: PersonId,
        last: Option<SymptomValue>,
    ) -> Option<(Option<SymptomValue>, f64)> {
        // People are `None` until they are infected and after their symptoms have improved.
        if let Some(symptoms) = last {
            // People become presymptomatic when they are infected.
            // If they are presymptomatic, we schedule their symptom development.
            if symptoms == SymptomValue::Presymptomatic {
                return Some(schedule_symptoms(self, context));
            }
            // Otherwise, person is currently experiencing symptoms, so schedule recovery.
            return Some(schedule_recovery(self, context));
        }
        None
    }
}

fn schedule_symptoms(data: &SymptomData, context: &Context) -> (Option<SymptomValue>, f64) {
    // Draw from the incubation period
    let time = context.sample(SymptomRng, |rng| {
        let i = data.incubation_period;
        if i.lower_bound < 0.0 {
            // Just draw from the vanilla log-normal
            let log_normal = statrs::distribution::LogNormal::new(i.mean, i.std_dev).unwrap();
            return rng.sample(log_normal);
        }
        // To be filled in...
        rng.gen_range(0.0..1.0)
    });
    // Assign this person the corresponding symptom category at the given time
    (Some(data.category), time)
}

fn schedule_recovery(data: &SymptomData, context: &Context) -> (Option<SymptomValue>, f64) {
    // Draw from the time to symptom improvement distribution
    let time = context.sample(SymptomRng, |rng| {
        let w = data.time_to_symptom_improvement;
        let random_cdf_value = rng.gen_range(0.0..1.0);
        // Convert a uniform value to a Weibull sample using inverse transform sampling
        // We do exactly this analysis in our Stan model.
        // See function `discrete_weibull_truncated_rng` in the Stan model. For completeness, here's
        // the sampling code (recall beta = shape, si_scale_inv = 1 / scale):
        // real log_q = -(si_scale_inv ^ beta);
        // real days = (log1m_exp(log(random_cdf_value) + discrete_weibull_lcdf(max_si | si_scale_inv, beta)) / log_q) ^ (1 / beta);
        // return ceil(days);
        let log_q = -((1.0 / w.scale).powf(w.shape));
        let days = (f64::ln_1p(
            -random_cdf_value
                * Weibull::new(1.0 / w.scale, w.shape)
                    .unwrap()
                    .cdf(w.upper_bound),
        ) / log_q)
            .powf(1.0 / w.shape);
        days.ceil()
    });
    // Schedule the person to recover from their symptoms (`Symptoms` = `None`) at the given time
    (None, time)
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let params = context.get_params();
    load_progression_library(
        context,
        Symptoms,
        params.symptom_progression_library.clone(),
    )?;

    event_subscriptions(context);
    Ok(())
}

fn event_subscriptions(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                context.set_person_property(
                    event.person_id,
                    Symptoms,
                    Some(SymptomValue::Presymptomatic),
                );
            }
        },
    );
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use super::{init, SymptomData, SymptomValue};
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{GlobalParams, ItineraryWriteFnType, LibraryType},
        property_progression_manager::Progression,
        rate_fns::load_rate_fns,
        symptom_progression::{
            event_subscriptions, schedule_recovery, schedule_symptoms, LowerTruncatedLogNormal,
            Symptoms, UpperTruncatedDiscreteWeibull,
        },
        Params,
    };

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt,
        PersonPropertyChangeEvent,
    };

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: LibraryType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: LibraryType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_schedule_symptoms() {
        let context = setup();
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: LowerTruncatedLogNormal::new(5.0, 1.0, 1.0).unwrap(),
            time_to_symptom_improvement: UpperTruncatedDiscreteWeibull::new(2.0, 3.0, 28.0)
                .unwrap(),
        };
        let symptoms = schedule_symptoms(&symptom_data, &context);
        assert_eq!(symptoms.0, Some(SymptomValue::Category1));
        assert!(symptoms.1 > 0.0); // Check that the time to symptoms is positive
    }

    #[test]
    fn test_schedule_recovery() {
        let context = setup();
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: LowerTruncatedLogNormal::new(5.0, 1.0, 1.0).unwrap(),
            time_to_symptom_improvement: UpperTruncatedDiscreteWeibull::new(2.0, 3.0, 28.0)
                .unwrap(),
        };
        let recovery = schedule_recovery(&symptom_data, &context);
        assert_eq!(recovery.0, None);
        assert!(recovery.1 > 0.0); // Check that the time to recovery is positive
    }

    #[test]
    fn test_progression_impl_symptom_data() {
        let symptom_data = SymptomData {
            category: SymptomValue::Category1,
            incubation_period: LowerTruncatedLogNormal::new(5.0, 1.0, 1.0).unwrap(),
            time_to_symptom_improvement: UpperTruncatedDiscreteWeibull::new(2.0, 3.0, 28.0)
                .unwrap(),
        };
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        let presymptomatic_next =
            symptom_data.next(&context, person, Some(SymptomValue::Presymptomatic));
        assert_eq!(
            presymptomatic_next.unwrap().0,
            Some(SymptomValue::Category1)
        );
        assert!(presymptomatic_next.unwrap().1 > 0.0); // Check that the time to symptoms is positive
        let category1_next = symptom_data.next(&context, person, Some(SymptomValue::Category1));
        assert!(category1_next.unwrap().0.is_none());
        assert!(category1_next.unwrap().1 > 0.0); // Check that the time to recovery is positive
        let none_next = symptom_data.next(&context, person, None);
        assert!(none_next.is_none());
    }

    #[test]
    fn test_event_subscriptions() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        event_subscriptions(&mut context);
        context.infect_person(person, None);
        context.execute();
        // The person should be presymptomatic
        assert_eq!(
            context.get_person_property(person, Symptoms),
            Some(SymptomValue::Presymptomatic)
        );
    }

    #[test]
    fn test_init() {
        let mut context = setup();
        let person = context.add_person(()).unwrap();
        init(&mut context).unwrap();
        context.infect_person(person, None);
        // At time 0, the person should become presymptomatic (because `event_subscriptions`)
        context.add_plan(0.0, move |ctx| {
            assert_eq!(
                ctx.get_person_property(person, Symptoms),
                Some(SymptomValue::Presymptomatic)
            );
        });
        // At some time, the person should be in one of the symptom categories. They should only
        // ever pass through one of the symptom categories. We don't know which one, because we
        // don't know what property progression the person is assigned.
        let assigned_category: Rc<RefCell<Option<SymptomValue>>> = Rc::new(RefCell::new(None));
        let assigned_category_clone = assigned_category.clone();
        context.subscribe_to_event(move |_, event: PersonPropertyChangeEvent<Symptoms>| {
            if let Some(symptoms) = event.current {
                if symptoms == SymptomValue::Presymptomatic {
                    // Person must be coming from None and going to Presymptomatic
                    assert!(event.previous.is_none());
                } else {
                    assert_eq!(event.previous, Some(SymptomValue::Presymptomatic));
                    *assigned_category_clone.borrow_mut() = Some(symptoms);
                }
            } else if event.current.is_none() {
                assert!(event.previous != Some(SymptomValue::Presymptomatic));
                assert_eq!(event.previous, *assigned_category.borrow());
            }
        });
        context.execute();
    }
}
