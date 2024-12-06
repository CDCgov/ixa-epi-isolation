use std::fmt::Debug;
use std::path::PathBuf;

use ixa::{
    define_data_plugin, define_global_property, define_rng, error::IxaError, Context,
    ContextGlobalPropertiesExt, ContextRandomExt,
};
use ndarray::linspace;
use serde::{Deserialize, Serialize};
use statrs::distribution::{Continuous, Weibull};

define_rng!(IncubationPeriodRng);
define_data_plugin!(IncubationPeriodTimes, Vec<f64>, Vec::new());

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub asymptomatic_probability: f64,
    pub incubation_period: [f64; 3],
    pub hospitalization_infection_probability: f64,
    pub time_to_hospitalization: f64,
    pub hospitalization_duration: f64,
    pub generation_interval: f64,
    pub report_period: f64,
    pub synth_population_file: PathBuf,
    pub population_periodic_report: String,
}

fn validate_inputs(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.r_0 < 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number.".to_string(),
        ));
    }
    if parameters.generation_interval <= 0.0 {
        return Err(IxaError::IxaError(
            "The generation interval must be positive.".to_string(),
        ));
    }
    if parameters.asymptomatic_probability < 0.0 || parameters.asymptomatic_probability > 1.0 {
        return Err(IxaError::IxaError(
            "The asymptomatic probability must be between 0 and 1.".to_string(),
        ));
    }
    if parameters.hospitalization_infection_probability < 0.0
        || parameters.hospitalization_infection_probability > 1.0
    {
        return Err(IxaError::IxaError(
            "The hospitalization infection probability must be between 0 and 1.".to_string(),
        ));
    }
    if parameters.incubation_period[0] <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period shape must be positive.".to_string(),
        ));
    }
    if parameters.incubation_period[1] <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period shape must be positive.".to_string(),
        ));
    }
    if parameters.time_to_hospitalization <= 0.0 {
        return Err(IxaError::IxaError(
            "The time to hospitalization must be positive.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

pub fn make_derived_parameters(context: &mut Context) {
    // Make the custom PMF of the incubation period distribution.
    // This is exactly what's plotted in Fig. 2B of Park et al., (2023) PNAS.
    let parameters = context.get_global_property_value(Parameters).unwrap();
    let shape = parameters.incubation_period[0];
    let scale = parameters.incubation_period[1];
    let growth_rate = parameters.incubation_period[2];
    let weibull = Weibull::new(shape, scale).unwrap();
    let mut scaled_probs_incubation_period_times: Vec<f64> = linspace(0.0, 23.0, 1000)
        .map(|t| weibull.pdf(t) * f64::exp(growth_rate * t))
        .collect();
    context
        .get_data_container_mut(IncubationPeriodTimes)
        .append(&mut scaled_probs_incubation_period_times);
}

pub trait ContextParametersExt {
    fn sample_incubation_period_time(&self) -> f64;
}

impl ContextParametersExt for Context {
    #[allow(clippy::cast_precision_loss)]
    fn sample_incubation_period_time(&self) -> f64 {
        let incubation_period_times = self.get_data_container(IncubationPeriodTimes).unwrap();
        let index = self.sample_weighted(IncubationPeriodRng, incubation_period_times);
        index as f64 / incubation_period_times.len() as f64 * 23.0
    }
}

#[cfg(test)]
mod test {
    use ixa::error::IxaError;

    use super::validate_inputs;
    use std::path::PathBuf;

    use crate::parameters::ParametersValues;

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            asymptomatic_probability: 0.2,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: 0.01,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "r_0 must be a non-negative number.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that r_0 validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_gi() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: 0.2,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: 0.01,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The generation interval must be positive.".to_string()),
            Some(ue) => panic!("Expected an error that the generation interval validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_incubation_period() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: 0.2,
            incubation_period: [-3.6, 1.5, 0.15],
            hospitalization_infection_probability: 0.01,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The incubation period shape must be positive.".to_string()),
            Some(ue) => panic!("Expected an error that the incubation period shape validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_asymptomatic_probability() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: -0.1,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: 0.01,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The asymptomatic probability must be between 0 and 1.".to_string()),
            Some(ue) => panic!("Expected an error that the asymptomatic probability validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: 1.1,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: 0.01,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The asymptomatic probability must be between 0 and 1.".to_string()),
            Some(ue) => panic!("Expected an error that the asymptomatic probability validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_hospitalization_infection_probability() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: 0.2,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: -0.1,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The hospitalization infection probability must be between 0 and 1.".to_string()),
            Some(ue) => panic!("Expected an error that the hospitalization infection probability validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            asymptomatic_probability: 0.2,
            incubation_period: [3.6, 1.5, 0.15],
            hospitalization_infection_probability: 1.1,
            time_to_hospitalization: 5.0,
            hospitalization_duration: 10.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The hospitalization infection probability must be between 0 and 1.".to_string()),
            Some(ue) => panic!("Expected an error that the hospitalization infection probability validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
    #[test]
    fn test_incubation_period_pmf() {
        // Mean of incubation period should be about exp(mu + 0.5 * sigma^2)
    }
}
