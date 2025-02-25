use std::fmt::Debug;
use std::path::PathBuf;

use ixa::{define_global_property, ContextGlobalPropertiesExt, IxaError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Rates {
    Constant(f64),
    Empirical(Vec<(f64, f64)>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Params {
    /// The number of infections we seed the population with.
    pub initial_infections: usize,
    /// The maximum run time of the simulation; even if there are still infections
    /// scheduled to occur, the simulation will stop at this time.
    pub max_time: f64,
    /// The random seed for the simulation.
    pub seed: u64,
    /// A library of infection rates to assign to infected people.
    pub rate_of_infection: Vec<Rates>,
    /// The duration of the infection in days
    pub infection_duration: f64,
    /// The period at which to report tabulated values
    pub report_period: f64,
    /// The path to the synthetic population file loaded in `population_loader`
    pub synth_population_file: PathBuf,
    /// The path to the transmission report file
    pub transmission_report_name: Option<String>,
}

fn validate_inputs(parameters: &Params) -> Result<(), IxaError> {
    if parameters.max_time < 0.0 {
        return Err(IxaError::IxaError(
            "The max simulation running time must be non-negative.".to_string(),
        ));
    }
    if parameters.infection_duration < 0.0 {
        return Err(IxaError::IxaError(
            "The duration of infection must be non-negative.".to_string(),
        ));
    }
    if parameters.report_period < 0.0 {
        return Err(IxaError::IxaError(
            "The report writing period must be non-negative.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(GlobalParams, Params, validate_inputs);

pub trait ContextParametersExt {
    fn get_params(&self) -> &Params;
}

impl ContextParametersExt for ixa::Context {
    fn get_params(&self) -> &Params {
        self.get_global_property_value(GlobalParams)
            .expect("Expected GlobalParams to be set")
    }
}

#[cfg(test)]
mod test {
    use ixa::{Context, ContextGlobalPropertiesExt, IxaError};

    use super::validate_inputs;
    use std::path::PathBuf;

    use crate::parameters::{ContextParametersExt, GlobalParams, Params, Rates};

    #[test]
    fn test_default_input_file() {
        let mut context = Context::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("input/input.json");
        context
            .load_global_properties(&path)
            .expect("Could not load input file");
        context.get_params();
    }

    #[test]
    fn test_get_params() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            rate_of_infection: vec![Rates::Constant(1.0)],
            infection_duration: 5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let &Params {
            initial_infections, ..
        } = context.get_params();
        assert_eq!(initial_infections, 1);
    }

    #[test]
    fn test_validate_infection_duration() {
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            rate_of_infection: vec![Rates::Constant(1.0)],
            infection_duration: -5.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The duration of infection must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the duration of infection validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_deserialization_rates() {
        let deserialized = serde_json::from_str::<Vec<Rates>>("[{\"Constant\": 1.0}, {\"Empirical\": [[0.0, 0.0], [1.0, 1.0]]}]").unwrap();
        assert_eq!(deserialized[0], Rates::Constant(1.0));
        assert_eq!(deserialized[1], Rates::Empirical(vec![(0.0, 0.0), (1.0, 1.0)]));
    }
}
