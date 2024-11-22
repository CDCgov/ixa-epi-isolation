use ixa::context::Context;
use ixa::global_properties::ContextGlobalPropertiesExt;
use std::fmt::Debug;
use std::path::Path;

use ixa::define_global_property;
use ixa::error::IxaError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub population: usize,
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub infection_duration: f64,
    pub generation_interval: f64,
    pub report_period: f64,
}
define_global_property!(Parameters, ParametersValues);

/// Check whether parameters provided in input json
/// are valid using our knowledge of allowable parameter
/// values. This prevents us from, say, loading the synth
/// pop only to realize that we accidentally set the GI to
/// be a negative number, which would cause a panic much
/// later in the code (when transmission happens).
fn validate(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.r_0 < 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number".to_string(),
        ));
    }
    // need to think about a better way of validation
    // in the long term that makes sense for an arbitrary GI
    if parameters.generation_interval <= 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number".to_string(),
        ));
    }
    Ok(())
}

pub fn init_parameters(context: &mut Context, file_path: &Path) -> Result<(), IxaError> {
    let parameters_json = context.load_parameters_from_json::<ParametersValues>(file_path)?;
    validate(&parameters_json)?;
    context.set_global_property_value(Parameters, parameters_json);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::parameters_loader::ParametersValues;

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            population: 1000,
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            infection_duration: 5.0,
            generation_interval: 5.0,
            report_period: 1.0,
        };
        assert!(validate(&parameters).is_err());
    }

    #[test]
    fn test_validate_gi() {
        let parameters = ParametersValues {
            population: 1000,
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            infection_duration: 5.0,
            generation_interval: 0.0,
            report_period: 1.0,
        };
        assert!(validate(&parameters).is_err());
    }
}

