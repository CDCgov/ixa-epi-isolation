use crate::parameters::{ContextParametersExt, Params};
use ixa::{info, Context, IxaError};
use serde::{Deserialize, Serialize};

pub mod incidence_report;
pub mod prevalence_report;
pub mod transmission_report;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReportParams {
    pub write: bool,
    pub filename: Option<String>,
    pub period: Option<f64>,
    pub reporting_delay: Option<f64>,
}

fn get_report_name(params: &ReportParams) -> Result<Option<&str>, IxaError> {
    if params.write {
        if let Some(name) = &params.filename {
            return Ok(Some(name));
        }

        return Err(IxaError::IxaError(
            "Reports must be provided with a name when write is set to true".to_string(),
        ));
    }

    if let Some(name) = &params.filename {
        info!("Report {name} is off but has associated values with name.");
    }
    Ok(None)
}

fn get_report_name_period_delay(
    params: &ReportParams,
) -> Result<Option<(&str, f64, f64)>, IxaError> {
    if let Some(name) = get_report_name(params)? {
        if let Some(period) = params.period {
            if period <= 0.0 {
                return Err(IxaError::IxaError(
                    format!("The report period must be greater than zero, found period {period} for {name} instead.")
                ));
            }
            let delay = params.reporting_delay.unwrap_or(0.0);
            return Ok(Some((name, period, delay)));
        }

        return Err(IxaError::IxaError(format!(
            "Report {name} requires a period but none provided."
        )));
    }
    Ok(None)
}

fn get_report_name_delay(params: &ReportParams) -> Result<Option<(&str, f64)>, IxaError> {
    if let Some(name) = get_report_name(params)? {
        let delay = params.reporting_delay.unwrap_or(0.0);
        return Ok(Some((name, delay)));
    }
    Ok(None)
}

/// # Errors
///
/// Will return `IxaError` if any report within the reports list cannot be added
/// or if the period for any periodic report is less than 0.0
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        prevalence_report,
        incidence_report,
        transmission_report,
        ..
    } = context.get_params().clone();
    let mut report_count = 0;

    if let Some((name, period, reporting_delay)) = get_report_name_period_delay(&prevalence_report)?
    {
        prevalence_report::init(context, name, period, reporting_delay)?;
        info!("Generating the prevalence report.");
        report_count += 1;
    }
    if let Some((name, period, reporting_delay)) = get_report_name_period_delay(&incidence_report)?
    {
        incidence_report::init(context, name, period, reporting_delay)?;
        info!("Generating the incidence report.");
        report_count += 1;
    }
    if let Some((name, reporting_delay)) = get_report_name_delay(&transmission_report)? {
        transmission_report::init(context, name, reporting_delay)?;
        info!("Generating the transmission report.");
        report_count += 1;
    }

    info!("Generating {report_count} report(s) in total.");

    Ok(())
}

#[cfg(test)]
mod test {

    use super::get_report_name_period_delay;
    use crate::reports::ReportParams;
    use crate::{
        parameters::{ContextParametersExt, Params},
        rate_fns::load_rate_fns,
    };
    use ixa::{Context, ContextGlobalPropertiesExt, ContextRandomExt, IxaError};
    use statrs::assert_almost_eq;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

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
    fn test_list_reports() {
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
                    "prevalence_report": {
                        "write": true,
                        "filename": "prevalence.csv",
                        "period": 1.0
                    },
                    "incidence_report": {
                        "write": true,
                        "filename": "incidence.csv",
                        "period": 2.0
                    },
                    "transmission_report": {
                        "write": true,
                        "filename": "transmission.csv"
                    },
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
        let Params {
            prevalence_report,
            incidence_report,
            transmission_report,
            ..
        } = context.get_params().clone();

        assert!(prevalence_report.write);
        assert_eq!(
            prevalence_report.filename,
            Some("prevalence.csv".to_string())
        );
        assert_eq!(prevalence_report.period, Some(1.0));

        assert!(incidence_report.write);
        assert_eq!(incidence_report.filename, Some("incidence.csv".to_string()));
        assert_eq!(incidence_report.period, Some(2.0));

        assert!(transmission_report.write);
        assert_eq!(
            transmission_report.filename,
            Some("transmission.csv".to_string())
        );
        assert_eq!(transmission_report.period, None);
    }

    #[test]
    fn test_get_period_report_name() {
        let name = "output.csv".to_string();
        let period = 3.0;

        let report = ReportParams {
            write: true,
            filename: Some(name.clone()),
            period: Some(period),
            reporting_delay: None,
        };

        if let Some((expect_name, expect_period, expected_delay)) =
            get_report_name_period_delay(&report).unwrap()
        {
            assert_eq!(name, *expect_name);
            assert_almost_eq!(period, expect_period, 0.0);
            assert_almost_eq!(0.0, expected_delay, 0.0);
        } else {
            panic!("Expected some value for the validated name and period");
        }
    }

    #[test]
    fn test_get_period_report_name_nowrite() {
        let name = "output.csv".to_string();
        let period = 3.0;

        let report = ReportParams {
            write: false,
            filename: Some(name),
            period: Some(period),
            reporting_delay: None,
        };

        assert_eq!(None, get_report_name_period_delay(&report).unwrap());
    }

    #[test]
    fn test_error_no_name() {
        let period = 3.0;

        let no_name_report = ReportParams {
            write: true,
            filename: None,
            period: Some(period),
            reporting_delay: None,
        };

        match get_report_name_period_delay(&no_name_report).err() {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Reports must be provided with a name when write is set to true".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error the report name is required. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead validation passed with no errors."),
        }
    }

    #[test]
    fn test_error_bad_period() {
        let name = "output.csv".to_string();
        let bad_period = 0.0;

        let bad_period_report = ReportParams {
            write: true,
            filename: Some(name),
            period: Some(bad_period),
            reporting_delay: None,
        };

        match get_report_name_period_delay(&bad_period_report).err() {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "The report period must be greater than zero, found period 0 for output.csv instead.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error the report name is required. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead validation passed with no errors."),
        }
    }
}
