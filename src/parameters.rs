use std::{
    collections::HashMap,
    fmt::Debug,
    fmt::{Display, Formatter},
    path::PathBuf,
};

use ixa::{define_global_property, ContextGlobalPropertiesExt, IxaError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateFnType {
    Constant { rate: f64, duration: f64 },
    EmpiricalFromFile { file: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressionLibraryType {
    EmpiricalFromFile { file: PathBuf },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum CoreSettingsTypes {
    Home { alpha: f64 },
    School { alpha: f64 },
    Workplace { alpha: f64 },
    CensusTract { alpha: f64 },
}

impl CoreSettingsTypes {
    fn get_alpha(&self) -> &f64 {
        match self {
            CoreSettingsTypes::Home { alpha }
            | CoreSettingsTypes::School { alpha }
            | CoreSettingsTypes::Workplace { alpha }
            | CoreSettingsTypes::CensusTract { alpha } => alpha,
        }
    }
}

impl Display for CoreSettingsTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreSettingsTypes::Home { .. } => write!(f, "Home"),
            CoreSettingsTypes::School { .. } => write!(f, "School"),
            CoreSettingsTypes::Workplace { .. } => write!(f, "Workplace"),
            CoreSettingsTypes::CensusTract { .. } => write!(f, "CensusTract"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ItineraryWriteFnType {
    /// Splits the ratio of infectiousness across all specified settings evenly
    /// Useful for when we need to create a new setting in tests and have people in that setting only
    SplitEvenly,
    /// Split the ratio of infectiousness across the core settings according to the provided proportions
    Split {
        home: f64,
        school: f64,
        workplace: f64,
        census_tract: f64,
    },
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
    /// A library of symptom progressions
    pub symptom_progression_library: Option<ProgressionLibraryType>,
    /// The period at which to report tabulated values
    pub report_period: f64,
    /// Setting properties, currently only the transmission modifier alpha values for each setting
    pub settings_properties: Vec<CoreSettingsTypes>,
    /// Rule set for writing itineraries
    pub itinerary_fn_type: ItineraryWriteFnType,
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

    let mut settings_map: HashMap<String, f64> = HashMap::new();

    for setting in &parameters.settings_properties {
        let setting_string = setting.to_string();
        let alpha = setting.get_alpha();
        if *alpha < 0.0 || *alpha > 1.0 {
            return Err(IxaError::IxaError(
                "The alpha values for each setting must be between 0 and 1, inclusive.".to_string(),
            ));
        }
        if settings_map.insert(setting_string, *alpha).is_some() {
            return Err(IxaError::IxaError(
                "Setting types must be uniquely associated with an alpha value.".to_string(),
            ));
        }
    }
    // Check that none of the setting proportions are below zero
    let setting_proportions = parameters.itinerary_fn_type;
    match setting_proportions {
        ItineraryWriteFnType::Split {
            home,
            school,
            workplace,
            census_tract,
        } => {
            if home < 0.0 || school < 0.0 || workplace < 0.0 || census_tract < 0.0 {
                return Err(IxaError::IxaError(
                    "The itinerary ratio for each setting must be non-negative.".to_string(),
                ));
            }
            if home == 0.0 && school == 0.0 && workplace == 0.0 && census_tract == 0.0 {
                return Err(IxaError::IxaError(
                    "At least one itinerary ratio must be greater than zero.".to_string(),
                ));
            }
        }
        ItineraryWriteFnType::SplitEvenly => {}
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
    use crate::parameters::{
        ContextParametersExt, GlobalParams, ItineraryWriteFnType, Params, RateFnType,
    };
    use std::path::PathBuf;

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
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::Split {
                home: 0.25,
                school: 0.25,
                workplace: 0.25,
                census_tract: 0.25,
            },
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
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::Split {
                home: 0.25,
                school: 0.25,
                workplace: 0.25,
                census_tract: 0.25,
            },
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
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::Split {
                home: 0.0,
                school: 0.0,
                workplace: 0.0,
                census_tract: 0.0,
            },
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
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::Split {
                home: -0.1,
                school: 0.0,
                workplace: 0.0,
                census_tract: 0.0,
            },
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
