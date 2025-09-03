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

// #[cfg(test)]
// mod test {

//     use crate::{
//         infectiousness_manager::InfectionContextExt, parameters::ContextParametersExt,
//         rate_fns::load_rate_fns,
//     };
//     use ixa::{
//         Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
//     };
//     use statrs::assert_almost_eq;
//     use std::path::PathBuf;
//     use tempfile::tempdir;

//     use super::TransmissionReport;
//     use std::fs::File;
//     use std::io::Write;

//     fn setup_context_from_str(params_json: &str) -> Context {
//         let temp_dir = tempdir().unwrap();
//         let dir = PathBuf::from(&temp_dir.path());
//         let file_path = dir.join("input.json");
//         let mut file = File::create(file_path.clone()).unwrap();
//         file.write_all(params_json.as_bytes()).unwrap();

//         let mut context = Context::new();
//         context.load_global_properties(&file_path).unwrap();
//         context.init_random(context.get_params().seed);
//         load_rate_fns(&mut context).unwrap();
//         context
//     }

//     #[test]
//     fn test_filled_transmission_report() {
//         let params_json = r#"
//             {
//                 "epi_isolation.GlobalParams": {
//                     "max_time": 200.0,
//                     "seed": 123,
//                     "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
//                     "initial_incidence": 0.01,
//                     "initial_recovered": 0.0,
//                     "report_period": 1.0,
//                     "proportion_asymptomatic": 0.0,
//                     "relative_infectiousness_asymptomatics": 0.0,
//                     "settings_properties": {},
//                     "synth_population_file": "input/people_test.csv",
//                     "reports": [
//                         "TransmissionReport": {
//                             "name": "output.csv"
//                         }
//                     ],
//                     "hospitalization_parameters": {
//                         "age_groups": [
//                             {"min": 0, "probability": 0.0},
//                             {"min": 19, "probability": 0.0},
//                             {"min": 65, "probability": 0.0}
//                         ],
//                         "mean_delay_to_hospitalization": 1.0,
//                         "mean_duration_of_hospitalization": 1.0,
//                         "hospital_incidence_report_name": "hospital_incidence_report.csv"
//                     }
//                 }
//             }
//         "#;
//         let context = setup_context_from_str(params_json);
//         let report_name = context.get_params().transmission_report_name.clone();
//         assert_eq!(report_name.unwrap(), "output.csv".to_string());
//     }

//     #[test]
//     fn test_generate_transmission_report() {
//         let params_json = r#"
//             {
//                 "epi_isolation.GlobalParams": {
//                     "max_time": 200.0,
//                     "seed": 123,
//                     "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
//                     "initial_incidence": 0.0,
//                     "initial_recovered": 0.0,
//                     "proportion_asymptomatic": 0.0,
//                     "relative_infectiousness_asymptomatics": 0.0,
//                     "report_period": 1.0,
//                     "settings_properties": {},
//                     "synth_population_file": "input/people_test.csv",
//                     "transmission_report_name": "output.csv",
//                     "hospitalization_parameters": {
//                         "age_groups": [
//                             {"min": 0, "probability": 0.0},
//                             {"min": 19, "probability": 0.0},
//                             {"min": 65, "probability": 0.0}
//                         ],
//                         "mean_delay_to_hospitalization": 1.0,
//                         "mean_duration_of_hospitalization": 1.0,
//                         "hospital_incidence_report_name": "hospital_incidence_report.csv"
//                     }
//                 }
//             }
//         "#;
//         let mut context = setup_context_from_str(params_json);

//         let temp_dir = tempdir().unwrap();
//         let path = PathBuf::from(&temp_dir.path());
//         let config = context.report_options();
//         config.directory(path.clone());

//         let source = context.add_person(()).unwrap();
//         let target = context.add_person(()).unwrap();
//         let setting_type = Some("test_setting");
//         let setting_id: Option<usize> = Some(1);
//         let infection_time = 1.0;

//         context.infect_person(source, None, None, None);
//         crate::transmission_report::init(&mut context).unwrap();

//         context.add_plan(infection_time, move |context| {
//             context.infect_person(target, Some(source), setting_type, setting_id);
//         });
//         context.execute();

//         let file_path = path.join(
//             context
//                 .get_params()
//                 .transmission_report_name
//                 .clone()
//                 .unwrap(),
//         );

//         assert!(file_path.exists());
//         std::mem::drop(context);

//         assert!(file_path.exists());
//         let mut reader = csv::Reader::from_path(file_path).unwrap();
//         let mut line_count = 0;
//         for result in reader.deserialize() {
//             let record: TransmissionReport = result.unwrap();
//             assert_almost_eq!(record.time, infection_time, 0.0);
//             assert_eq!(record.target_id, target);
//             assert_eq!(record.infected_by.unwrap(), source);
//             assert_eq!(
//                 record.infection_setting_type,
//                 Some("test_setting".to_string())
//             );
//             assert_eq!(record.infection_setting_id, setting_id);
//             line_count += 1;
//         }
//         assert_eq!(line_count, 1);
//     }
// }
