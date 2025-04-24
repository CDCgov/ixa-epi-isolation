use std::{fmt::Debug, path::PathBuf};

use ixa::{define_global_property, ContextGlobalPropertiesExt, IxaError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateFnType {
    Constant { rate: f64, duration: f64 },
    EmpiricalFromFile { file: PathBuf },
}

pub const CORE_SETTING_VARIANTS: usize = 5;
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum CoreSettingsTypes {
    Home { alpha: f64 },
    School { alpha: f64 },
    Workplace { alpha: f64 },
    CensusTract { alpha: f64 },
    Global { alpha: f64 },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ItineraryWriteFnType {
    SplitEvenly,
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
    pub infectiousness_rate_fn: RateFnType,
    /// The period at which to report tabulated values
    pub report_period: f64,
    /// Setting properties, currently only the transmission modifier alpha values for each setting
    pub settings_properties: Vec<CoreSettingsTypes>,
    /// Rule set for writing itineraries
    pub itinerary_write_fn: ItineraryWriteFnType,
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
    if parameters.report_period < 0.0 {
        return Err(IxaError::IxaError(
            "The report writing period must be non-negative.".to_string(),
        ));
    }

    let mut variant_counts = [0; CORE_SETTING_VARIANTS];
    for setting in &parameters.settings_properties {
        let alpha_value;
        match setting {
            CoreSettingsTypes::Home { alpha } => {
                alpha_value = alpha;
                variant_counts[0] += 1;
            }
            CoreSettingsTypes::School { alpha } => {
                alpha_value = alpha;
                variant_counts[1] += 1;
            }
            CoreSettingsTypes::Workplace { alpha } => {
                alpha_value = alpha;
                variant_counts[2] += 1;
            }
            CoreSettingsTypes::CensusTract { alpha } => {
                alpha_value = alpha;
                variant_counts[3] += 1;
            }
            CoreSettingsTypes::Global { alpha } => {
                alpha_value = alpha;
                variant_counts[4] += 1;
            }
        }

        if *alpha_value < 0.0 {
            return Err(IxaError::IxaError(
                "The alpha values for each setting must be non-negative.".to_string(),
            ));
        }
    }

    for count in variant_counts {
        if count > 1 {
            return Err(IxaError::IxaError(
                "Each setting type can only have one alpha value.".to_string(),
            ));
        }
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

    use crate::parameters::{
        ContextParametersExt, GlobalParams, ItineraryWriteFnType, Params, RateFnType,
    };

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
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_write_fn: ItineraryWriteFnType::SplitEvenly,
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
    fn test_validate_max_time() {
        let parameters = Params {
            initial_infections: 1,
            max_time: -100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_write_fn: ItineraryWriteFnType::SplitEvenly,
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The max simulation running time must be non-negative.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the max simulation running time validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_deserialization_rates() {
        let deserialized = serde_json::from_str::<RateFnType>(
            "{\"Constant\": {\"rate\": 1.0, \"duration\": 5.0}}",
        )
        .unwrap();
        assert_eq!(
            deserialized,
            RateFnType::Constant {
                rate: 1.0,
                duration: 5.0
            }
        );
    }
}
