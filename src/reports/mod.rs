use crate::parameters::{ContextParametersExt, Params};
use ixa::{info, Context, IxaError};
use serde::{Deserialize, Serialize};

pub mod incidence_report;
pub mod prevalence_report;
pub mod transmission_report;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ReportType {
    PrevalenceReport { name: String, period: f64 },
    IncidenceReport { name: String, period: f64 },
    TransmissionReport { name: String },
}

/// # Errors
///
/// Will return `IxaError` if any report within the reports list cannot be added
/// or if the period for any periodic report is less than 0.0
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params { reports, .. } = context.get_params().clone();

    // Should at least one report be required?
    if reports.is_empty() {
        info!("No reports are being generated.");
    }

    for report in &reports {
        match report {
            ReportType::PrevalenceReport { name, period } => {
                if period < &0.0 {
                    return Err(IxaError::IxaError(
                        "The prevalence report writing period must be non-negative.".to_string(),
                    ));
                }
                prevalence_report::init(context, name.as_str(), *period)?;
            }
            ReportType::IncidenceReport { name, period } => {
                if period < &0.0 {
                    return Err(IxaError::IxaError(
                        "The incidence report writing period must be non-negative.".to_string(),
                    ));
                }
                incidence_report::init(context, name.as_str(), *period)?;
            }
            ReportType::TransmissionReport { name } => {
                transmission_report::init(context, name.as_str())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use crate::reports::ReportType;
    use crate::{
        parameters::{ContextParametersExt, Params},
        rate_fns::load_rate_fns,
    };
    use ixa::{Context, ContextGlobalPropertiesExt, ContextRandomExt};
    use statrs::assert_almost_eq;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use std::fs::File;
    use std::io::Write;

    fn setup_context_from_str(params_json: &str) -> Context {
        let temp_dir = tempdir().unwrap();
        let dir = PathBuf::from(&temp_dir.path());
        let file_path = dir.join("input.json");
        let mut file = File::create(file_path.clone()).unwrap();
        file.write_all(params_json.as_bytes()).unwrap();

        let mut context = Context::new();
        context.load_global_properties(&file_path).unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_filled_transmission_report() {
        let params_json = r#"
            {
                "epi_isolation.GlobalParams": {
                    "max_time": 200.0,
                    "seed": 123,
                    "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
                    "initial_incidence": 0.01,
                    "initial_recovered": 0.0,
                    "proportion_asymptomatic": 0.0,
                    "relative_infectiousness_asymptomatics": 0.0,
                    "settings_properties": {},
                    "synth_population_file": "input/people_test.csv",
                    "reports": [
                        {"TransmissionReport": {
                            "name": "transmission.csv"
                        }},
                        {"PrevalenceReport": {
                            "name": "prevalence.csv",
                            "period": 1.0
                        }},
                        {"IncidenceReport": {
                            "name": "incidence.csv",
                            "period": 2.0
                        }}
                    ],
                    "hospitalization_parameters": {
                        "age_groups": [
                            {"min": 0, "probability": 0.0},
                            {"min": 19, "probability": 0.0},
                            {"min": 65, "probability": 0.0}
                        ],
                        "mean_delay_to_hospitalization": 1.0,
                        "mean_duration_of_hospitalization": 1.0
                    }
                }
            }
        "#;
        let context = setup_context_from_str(params_json);
        let Params { reports, .. } = context.get_params();
        assert_eq!(reports.len(), 3);
        for report in reports {
            match report {
                ReportType::TransmissionReport { name } => {
                    assert_eq!(*name, "transmission.csv".to_string());
                }
                ReportType::PrevalenceReport { name, period } => {
                    assert_eq!(*name, "prevalence.csv".to_string());
                    assert_almost_eq!(*period, 1.0, 0.0);
                }
                ReportType::IncidenceReport { name, period } => {
                    assert_eq!(*name, "incidence.csv".to_string());
                    assert_almost_eq!(*period, 2.0, 0.0);
                }
            }
        }
    }
}
