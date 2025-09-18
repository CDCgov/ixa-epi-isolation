use std::{fmt::Debug, path::PathBuf};

use ixa::{
    define_global_property, Context, ContextGlobalPropertiesExt, HashMap, HashMapExt, IxaError,
    PluginContext,
};
use serde::{Deserialize, Serialize};

use crate::policies::{validate_guidance_policy, Policies};
use crate::reports::ReportParams;
use crate::{hospitalizations::HospitalAgeGroups, settings::SettingProperties};

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

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct FacemaskParameters {
    pub facemask_efficacy: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HospitalizationParameters {
    /// The mean of the delay distribution to hospitalization.
    pub mean_delay_to_hospitalization: f64,
    /// The mean of the duration of hospitalization.
    pub mean_duration_of_hospitalization: f64,
    /// Age groups for hospitalization probabilities.
    pub age_groups: Vec<HospitalAgeGroups>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Params {
    /// The proportion of initial people who are infectious when we seed the population.
    pub initial_incidence: f64,
    /// The proportion of people that are initially recovered (fully immune to disease).
    pub initial_recovered: f64,
    /// The maximum run time of the simulation; even if there are still infections
    /// scheduled to occur, the simulation will stop at this time.
    pub max_time: f64,
    /// The random seed for the simulation.
    pub seed: u64,
    /// A library of infection rates to assign to infected people.
    pub infectiousness_rate_fn: RateFnType,
    /// A library of symptom progressions
    pub symptom_progression_library: Option<ProgressionLibraryType>,
    /// Proportion of infected individuals who do not develop symptoms
    pub proportion_asymptomatic: f64,
    /// Asymptomatic individuals are less infectious than symptomatic individuals
    pub relative_infectiousness_asymptomatics: f64,
    /// Setting properties by setting type
    pub settings_properties: HashMap<CoreSettingsTypes, SettingProperties>,
    /// The path to the synthetic population file loaded in `population_loader`
    pub synth_population_file: PathBuf,
    /// Prevalence report with a period and name required
    pub prevalence_report: ReportParams,
    /// Incidence report with a period and name required
    pub incidence_report: ReportParams,
    /// Transmission report with a name required
    pub transmission_report: ReportParams,
    /// Facemask parameters
    /// The reduction in transmission associated with wearing a facemask.
    pub facemask_parameters: Option<FacemaskParameters>,
    /// Hospitalization parameters contain the probability of hospitalization by age group
    /// The mean of the delay distribution to hospitalization, and the mean of the duration of hospitalization.
    pub hospitalization_parameters: HospitalizationParameters,
    /// Guidance Policy
    /// Specifies the policy guidance to use for interventions, defaulting to None
    /// Enum variants should contain structs with policy-relevant data values
    pub guidance_policy: Option<Policies>,
    /// Any profiling data will be written to `{profiling_data_path}.json`
    pub profiling_data_path: Option<String>,
}

// Any default parameters must be specified here
// Please provide in-line justification for irregular defaults,
// such as: non-zero floats/integers, Some() options, and non-empty HashMaps
impl Default for Params {
    fn default() -> Self {
        Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 0.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            // Asymptomatics, if included, should act as symptomatics unless otherwise specified
            relative_infectiousness_asymptomatics: 1.0,
            prevalence_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            incidence_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            transmission_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            settings_properties: HashMap::new(),
            synth_population_file: PathBuf::new(),
            facemask_parameters: None,
            hospitalization_parameters: HospitalizationParameters {
                mean_delay_to_hospitalization: 0.0,
                mean_duration_of_hospitalization: 0.0,
                age_groups: vec![HospitalAgeGroups {
                    min: 0,
                    probability: 0.0,
                }],
            },
            guidance_policy: None,
            profiling_data_path: None,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn validate_inputs(parameters: &Params) -> Result<(), IxaError> {
    if parameters.max_time < 0.0 {
        return Err(IxaError::IxaError(
            "The max simulation running time must be non-negative.".to_string(),
        ));
    }
    // Initial conditions
    if !(0.0..=1.0).contains(&parameters.initial_incidence) {
        return Err(IxaError::IxaError(
            "The initial incidence must be between 0 and 1, inclusive.".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&parameters.initial_recovered) {
        return Err(IxaError::IxaError(
            "The initial recovered proportion must be between 0 and 1, inclusive.".to_string(),
        ));
    }
    // The sum of the initial incidence and initial recovered must be less than or equal to 1.
    if parameters.initial_incidence + parameters.initial_recovered >= 1.0 {
        return Err(IxaError::IxaError(
            "The sum of the initial incidence and initial recovered proportions must be less than or equal to 1."
                .to_string(),
        ));
    }

    // Check the infectiousness rate function
    match parameters.infectiousness_rate_fn {
        RateFnType::Constant { rate, duration } => {
            if rate < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness rate must be non-negative.".to_string(),
                ));
            }
            if duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness duration must be non-negative.".to_string(),
                ));
            }
        }
        RateFnType::EmpiricalFromFile { scale, .. } => {
            if scale < 0.0 {
                return Err(IxaError::IxaError(
                    "The empirical rate function infectiousness scale must be non-negative."
                        .to_string(),
                ));
            }
        }
    }

    // The policies module contains it's own validation function based on a match statement for the enum variant
    validate_guidance_policy(parameters.guidance_policy)?;

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

    // Check asymptomatic parameters
    if !(0.0..=1.0).contains(&parameters.proportion_asymptomatic) {
        return Err(IxaError::IxaError("The proportion of infected individuals who are asymptomatic must be between 0 and 1, inclusive.".to_string()));
    }
    if !(0.0..=1.0).contains(&parameters.relative_infectiousness_asymptomatics) {
        return Err(IxaError::IxaError("The relative infectiousness of asymptomatic individuals must be between 0 and 1, inclusive.".to_string()));
    }
    if let Some(facemask_parameters) = parameters.facemask_parameters {
        if !(0.0..=1.0).contains(&facemask_parameters.facemask_efficacy) {
            return Err(IxaError::IxaError(
                "The facemask probability must be between 0 and 1, inclusive.".to_string(),
            ));
        }
    }

    let hospitalization_parameters = &parameters.hospitalization_parameters;
    if hospitalization_parameters.mean_delay_to_hospitalization < 0.0 {
        return Err(IxaError::IxaError(
            "The mean delay to hospitalization must be non-negative.".to_string(),
        ));
    }
    if hospitalization_parameters.mean_duration_of_hospitalization < 0.0 {
        return Err(IxaError::IxaError(
            "The mean duration of hospitalization must be non-negative.".to_string(),
        ));
    }
    if hospitalization_parameters.age_groups.is_empty() {
        return Err(IxaError::IxaError(
            "There must be at least one age group for hospitalization probabilities.".to_string(),
        ));
    }
    for i in 1..hospitalization_parameters.age_groups.len() - 1 {
        let group = &hospitalization_parameters.age_groups[i - 1];

        let next_group = &hospitalization_parameters.age_groups[i];
        if group.min >= next_group.min {
            return Err(IxaError::IxaError(
                "Hospitalization age groups must be ordered by minimum age.".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&group.probability) {
            return Err(IxaError::IxaError(
                "The probability of hospitalization in each age group must be between 0 and 1, inclusive."
                    .to_string(),
            ));
        }
    }
    if hospitalization_parameters.age_groups[0].min != 0 {
        return Err(IxaError::IxaError(
            "The first age group for hospitalization probabilities must start at 0.".to_string(),
        ));
    }

    Ok(())
}

define_global_property!(GlobalParams, Params, validate_inputs);

pub trait ContextParametersExt: PluginContext + ContextGlobalPropertiesExt {
    fn get_params(&self) -> &Params {
        self.get_global_property_value(GlobalParams)
            .expect("Expected GlobalParams to be set")
    }
}
impl ContextParametersExt for Context {}

#[cfg(test)]
mod test {
    use ixa::{Context, ContextGlobalPropertiesExt, HashMap, IxaError};
    use statrs::assert_almost_eq;

    use super::{validate_inputs, CoreSettingsTypes, ItinerarySpecificationType};
    use crate::{
        parameters::{ContextParametersExt, GlobalParams, Params, RateFnType},
        settings::SettingProperties,
    };

    #[test]
    fn test_standard_input_file() {
        let mut context = Context::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("input/input.json");
        context
            .load_global_properties(&path)
            .expect("Could not load input file");
        context.get_params();
    }

    #[test]
    fn test_default_rate_fn_type() {
        let Params {
            infectiousness_rate_fn,
            ..
        } = Params::default();

        assert_eq!(
            infectiousness_rate_fn,
            RateFnType::Constant {
                rate: 1.0,
                duration: 5.0
            }
        );
    }

    #[test]
    fn test_get_params() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let &Params {
            initial_incidence, ..
        } = context.get_params();
        assert_almost_eq!(initial_incidence, 0.1, 0.0);
    }

    #[test]
    fn test_validate_max_time() {
        let parameters = Params {
            max_time: -100.0,
            ..Default::default()
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
            settings_properties: HashMap::from_iter(
                [
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
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
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
            settings_properties: HashMap::from_iter(
                [
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
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
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
            settings_properties: HashMap::from_iter(
                [
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
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        assert!(e.is_none(), "Expected no error, but got: {e:?}");
    }

    #[test]
    fn test_validation_itinerary_one_some_zero_rest_none() {
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
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
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
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
    fn test_proportion_asymptomatic() {
        let get_parameters = |proportion_asymptomatic| Params {
            proportion_asymptomatic,
            ..Default::default()
        };
        // Should pass
        let parameters = get_parameters(1.0);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for proportion_asymptomatic = 1.0");
        // Should pass
        let parameters = get_parameters(0.0);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for proportion_asymptomatic = 0.0");
        // Should pass
        let parameters = get_parameters(0.5);
        validate_inputs(&parameters)
            .expect("Expected validation to pass for proportion_asymptomatic = 0.5");
        // Should fail
        let parameters = get_parameters(-1.0);
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The proportion of infected individuals who are asymptomatic must be between 0 and 1, inclusive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the proportion of asymptomatic individuals validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
        // Should fail
        let parameters = get_parameters(1.1);
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "The proportion of infected individuals who are asymptomatic must be between 0 and 1, inclusive.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the fraction of asymptomatic individuals validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
