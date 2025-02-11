use std::fmt::Debug;
use std::path::PathBuf;

use ixa::{define_global_property, IxaError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    /// The number of infections we seed the population with.
    pub initial_infections: usize,
    /// The maximum run time of the simulation; even if there are still infections
    /// scheduled to occur, the simulation will stop at this time.
    pub max_time: f64,
    /// The random seed for the simulation.
    pub seed: u64,
    /// A global scale factor on the intrinsic infectiousness rate of each person
    pub global_transmissibility: f64,
    /// The duration of the infection in days
    pub infection_duration: f64,
    /// The period at which to report tabulated values
    pub report_period: f64,
    /// The path to the synthetic population file loaded in `population_loader`
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
