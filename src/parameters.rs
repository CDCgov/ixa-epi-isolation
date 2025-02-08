use std::fmt::Debug;
use std::path::PathBuf;

use ixa::{define_global_property, IxaError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub initial_infections: usize,
    pub max_time: f64,
    pub seed: u64,
    pub global_transmissibility: f64,
    pub infection_duration: f64,
    pub report_period: f64,
    pub synth_population_file: PathBuf,
}

fn validate_inputs(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.global_transmissibility < 0.0 {
        return Err(IxaError::IxaError(
            "global_transmissibility must be non-negative.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

#[cfg(test)]
mod test {
    use ixa::IxaError;

    use super::validate_inputs;
    use std::path::PathBuf;

    use crate::parameters::ParametersValues;

    #[test]
    fn test_validate_global_transmissibility() {
        let parameters = ParametersValues {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            global_transmissibility: -1.0,
            infection_duration: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "global_transmissibility must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that global_transmissibility validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_() {
        let parameters = ParametersValues {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            global_transmissibility: -1.0,
            infection_duration: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "global_transmissibility must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that global_transmissibility validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
