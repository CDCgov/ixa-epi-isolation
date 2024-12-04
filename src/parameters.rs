use std::fmt::Debug;
use std::path::PathBuf;

use ixa::{define_global_property, error::IxaError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub infection_duration: f64,
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
    Ok(())
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

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
            infection_duration: 5.0,
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
            infection_duration: 5.0,
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
}
