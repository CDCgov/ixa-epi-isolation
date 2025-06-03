use std::{collections::HashMap, fmt::Debug, path::PathBuf};

use ixa::{define_global_property, ContextGlobalPropertiesExt, IxaError};
use serde::{Deserialize, Serialize};

use crate::settings::SettingProperties;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateFnType {
    /// A constant rate of infectiousness (constant hazard -> exponential waiting times) for a given
    /// duration.
    Constant { rate: f64, duration: f64 },
    /// A library of empirical rate functions read in from a file.
    EmpiricalFromFile {
        /// The path to the library of empirical rates with columns, `id`, `time`, and `value`.
        file: PathBuf,
        /// Empirical rate functions are specified as hazard rates. However, the specified hazard
        /// rates are relative rather than absolute (unlike the constant rate of infectiousness
        /// which has an absolute rate of infection). We need a scale factor (that is often
        /// calibrated) to convert the relative hazard rates to absolute rates of infection.
        scale: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressionLibraryType {
    EmpiricalFromFile { file: PathBuf },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub enum CoreSettingsTypes {
    Home,
    School,
    Workplace,
    CensusTract,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ItinerarySpecificationType {
    Constant { ratio: f64 },
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
    /// A library of infection rates to assign to infected people. Used for asymptomatics as well if
    /// no separate asymptomatic rate function is provided.
    pub infectiousness_rate_fn: RateFnType,
    /// A library of symptom progressions.
    pub symptom_progression_library: Option<ProgressionLibraryType>,
    /// The fraction of infected individuals that are asymptomatic.
    pub fraction_asymptomatic: f64,
    /// The rate functions for asymptomatic individuals, if different from those for symptomatics.
    pub asymptomatic_rate_fn: Option<RateFnType>,
    /// The period at which to report tabulated values
    pub report_period: f64,
    /// Setting properties by setting type
    pub settings_properties: HashMap<CoreSettingsTypes, SettingProperties>,
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

    // Check the infectiousness rate functions
    check_rate_fn(&parameters.infectiousness_rate_fn)?;

    if let Some(asymptomatic_rate_fn) = &parameters.asymptomatic_rate_fn {
        check_rate_fn(asymptomatic_rate_fn)?;
    }

    if !(0.0..=1.0).contains(&parameters.fraction_asymptomatic) {
        return Err(IxaError::IxaError(
            "The fraction of asymptomatic individuals must be between 0 and 1, inclusive."
                .to_string(),
        ));
    }

    // If all the itinerary ratios are None, we can't validate them.
    // If some of them are zero and the rest are none, we still shouldn't fail.
    // We only want to fail when all of them are 0.
    // Instead of holding the itinerary ratios in a vector, we sum them because we error if they
    // are negative, so if their sum is 0.0, they must all be 0.0.
    let mut itinerary_ratio_sum = None;
    let mut some_none = false;

    for setting in parameters.settings_properties.values() {
        let alpha = setting.alpha;
        let itinerary_ratio = setting
            .itinerary_specification
            .map(|ItinerarySpecificationType::Constant { ratio }| ratio);
        // Check alpha
        if !(0.0..=1.0).contains(&alpha) {
            return Err(IxaError::IxaError(
                "The alpha values for each setting must be between 0 and 1, inclusive.".to_string(),
            ));
        }
        // Check itinerary ratio
        if let Some(itinerary_ratio) = itinerary_ratio {
            if itinerary_ratio < 0.0 {
                return Err(IxaError::IxaError(
                    "The itinerary ratio for each setting must be non-negative.".to_string(),
                ));
            }
            if let Some(sum) = itinerary_ratio_sum {
                itinerary_ratio_sum = Some(sum + itinerary_ratio);
            } else {
                itinerary_ratio_sum = Some(itinerary_ratio);
            }
        } else {
            // Means that we have a none variant, so even if the sum is 0.0, we can't error because
            // we don't know how the None itinerary will be specified in the code.
            some_none = true;
        }
    }
    if let Some(itinerary_ratio_sum) = itinerary_ratio_sum {
        if !some_none && itinerary_ratio_sum == 0.0 {
            return Err(IxaError::IxaError(
                "At least one itinerary ratio must be greater than zero.".to_string(),
            ));
        }
    }
    Ok(())
}

fn check_rate_fn(rate_fn: &RateFnType) -> Result<(), IxaError> {
    match rate_fn {
        RateFnType::Constant { rate, duration } => {
            if *rate < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness rate must be non-negative.".to_string(),
                ));
            }
            if *duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness duration must be non-negative.".to_string(),
                ));
            }
        }
        RateFnType::EmpiricalFromFile { scale, .. } => {
            if *scale < 0.0 {
                return Err(IxaError::IxaError(
                    "The empirical rate function infectiousness scale must be non-negative."
                        .to_string(),
                ));
            }
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

    use super::{validate_inputs, CoreSettingsTypes, ItinerarySpecificationType};
    use crate::{
        parameters::{ContextParametersExt, GlobalParams, Params, RateFnType},
        settings::SettingProperties,
    };
    use std::{collections::HashMap, path::PathBuf};

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
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
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
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
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
    fn test_validate_split_zeros() {
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 0.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::School,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 0.0,
                        }),
                    },
                ),
            ]),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "At least one itinerary ratio must be greater than zero.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that at least one itinerary ratio must be greater than zero. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_split_negative() {
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: -0.1,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::School,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 0.0,
                        }),
                    },
                ),
            ]),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "The itinerary ratio for each setting must be non-negative.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that itinerary ratios cannot be negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validation_itinerary_all_none() {
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: None,
                    },
                ),
                (
                    CoreSettingsTypes::School,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: None,
                    },
                ),
            ]),
        };
        let e = validate_inputs(&parameters).err();
        assert!(e.is_none(), "Expected no error, but got: {e:?}");
    }

    #[test]
    fn test_validation_itinerary_one_some_zero_rest_none() {
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.5,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 0.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::School,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: None,
                    },
                ),
            ]),
        };
        let e = validate_inputs(&parameters).err();
        assert!(e.is_none(), "Expected no error, but got: {e:?}");
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

    #[test]
    fn test_fraction_asymptomatic() {
        let get_parameters = |fraction_asymptomatic| Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        // Should pass
        let parameters = get_parameters(1.0);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for fraction_asymptomatic = 1.0");
        // Should pass
        let parameters = get_parameters(0.0);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for fraction_asymptomatic = 0.0");
        // Should pass
        let parameters = get_parameters(0.5);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for fraction_asymptomatic = 0.5");
        // Should fail
        let parameters = get_parameters(-1.0);
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The fraction of asymptomatic individuals must be between 0 and 1, inclusive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the fraction of asymptomatic individuals validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
        // Should fail
        let parameters = get_parameters(1.1);
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The fraction of asymptomatic individuals must be between 0 and 1, inclusive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the fraction of asymptomatic individuals validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
