use crate::{
    hospitalizations::Hospitalized,
    parameters::{ContextParametersExt, HospitalizationReportType},
    population_loader::Age,
};
use ixa::{
    define_report, info, report::ContextReportExt, Context, ContextPeopleExt, IxaError, PersonId,
    PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct HospitalIncidenceReport {
    time: f64,
    person_id: PersonId,
    age: u8,
    status: String,
}

define_report!(HospitalIncidenceReport);

fn record_hospital_incidence_event(
    context: &mut Context,
    person_id: PersonId,
    age: u8,
    status: String,
) {
    context.send_report(HospitalIncidenceReport {
        time: context.get_current_time(),
        person_id,
        age,
        status,
    });
}

fn create_hospital_incidence_report(
    context: &mut Context,
    file_name: &str,
    report_type: HospitalizationReportType,
) -> Result<(), IxaError> {
    match report_type {
        HospitalizationReportType::Incidence => {
            context.add_report::<HospitalIncidenceReport>(file_name)?;
            context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(
                |context, event| {
                    let age = context.get_person_property(event.person_id, Age);
                    if event.current {
                        record_hospital_incidence_event(
                            context,
                            event.person_id,
                            age,
                            "Arrival".to_string(),
                        );
                    }
                },
            );
        }
        HospitalizationReportType::Prevalence => {
            context.add_report::<HospitalIncidenceReport>(file_name)?;
            context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(
                |context, event| {
                    let age = context.get_person_property(event.person_id, Age);
                    if event.current {
                        record_hospital_incidence_event(
                            context,
                            event.person_id,
                            age,
                            "Arrival".to_string(),
                        );
                    } else {
                        record_hospital_incidence_event(
                            context,
                            event.person_id,
                            age,
                            "Departure".to_string(),
                        );
                    }
                },
            );
        }
        HospitalizationReportType::None => (),
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let report_name = context
        .get_params()
        .hospitalization_parameters
        .as_ref()
        .map(|hospital_parameters| hospital_parameters.hospital_incidence_report_name.clone());
    let report_type = context
        .get_params()
        .hospitalization_parameters
        .as_ref()
        .map_or(HospitalizationReportType::None, |hospital_parameters| {
            hospital_parameters.report_type
        });
    if let Some(report) = report_name {
        create_hospital_incidence_report(context, &report, report_type)?;
    } else {
        info!("No hospital incidence report name provided. Skipping hospital incidence report creation");
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use crate::hospitalizations::Hospitalized;
    use crate::population_loader::Age;
    use crate::{parameters::ContextParametersExt, rate_fns::load_rate_fns};
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    };
    use statrs::assert_almost_eq;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use super::HospitalIncidenceReport;
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
    fn test_empty_hospital_incidence_report() {
        let params_json = r#"
            {
                "epi_isolation.GlobalParams": {
                "max_time": 200.0,
                "seed": 123,
                "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
                "initial_incidence": 0.01,
                "initial_recovered": 0.0,
                "report_period": 1.0,
                "proportion_asymptomatic": 0.0,
                "relative_infectiousness_asymptomatics": 0.0,
                "settings_properties": {},
                "synth_population_file": "input/people_test.csv"
                }
            }
        "#;
        let context = setup_context_from_str(params_json);
        let report_name = context.get_params().transmission_report_name.clone();
        assert!(report_name.is_none());
    }

    #[test]
    fn test_filled_hospital_incidence_report() {
        let params_json = r#"
            {
                "epi_isolation.GlobalParams": {
                "max_time": 200.0,
                "seed": 123,
                "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
                "initial_incidence": 0.01,
                "initial_recovered": 0.0,
                "report_period": 1.0,
                "proportion_asymptomatic": 0.0,
                "relative_infectiousness_asymptomatics": 0.0,
                "settings_properties": {},
                "synth_population_file": "input/people_test.csv",
                "hospitalization_parameters": {
                    "age_groups": [
                        {"min": 0, "max": 18, "probability": 0.0001},
                        {"min": 19, "max": 64, "probability": 0.0001},
                        {"min": 65, "max": 120, "probability": 0.0001}
                    ],
                    "mean_delay_to_hospitalization": 1.0,
                    "mean_duration_of_hospitalization": 1.0,
                    "hospital_incidence_report_name": "hospital_incidence_report.csv",
                    "report_type": "Incidence"
                }
                }
            }
        "#;
        let context = setup_context_from_str(params_json);
        let report_name = context
            .get_params()
            .hospitalization_parameters
            .as_ref()
            .unwrap()
            .hospital_incidence_report_name
            .clone();
        assert_eq!(report_name, "hospital_incidence_report.csv".to_string());
    }

    #[test]
    fn test_generate_hospital_incidence_report() {
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
                "report_period": 1.0,
                "settings_properties": {},
                "synth_population_file": "input/people_test.csv",
                "hospitalization_parameters": {
                    "age_groups": [
                        {"min": 0, "max": 18, "probability": 0.0001},
                        {"min": 19, "max": 64, "probability": 0.0001},
                        {"min": 65, "max": 120, "probability": 0.0001}
                    ],
                    "mean_delay_to_hospitalization": 1.0,
                    "mean_duration_of_hospitalization": 1.0,
                    "hospital_incidence_report_name": "hospital_incidence_report.csv",
                    "report_type": "Incidence"
                }
                }
            }
        "#;
        let mut context = setup_context_from_str(params_json);

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        crate::hospital_incidence_report::init(&mut context).unwrap();

        let person_id = context.add_person((Age, 0)).unwrap();
        let hospitalization_time = 1.0;

        context.add_plan(hospitalization_time, move |context| {
            context.set_person_property(person_id, Hospitalized, true);
        });
        context.execute();

        let file_path = path.join("hospital_incidence_report.csv");

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        for result in reader.deserialize() {
            let record: HospitalIncidenceReport = result.unwrap();
            assert_almost_eq!(record.time, hospitalization_time, 0.0);
            assert_eq!(record.person_id, person_id);
            assert_eq!(record.age, 0);
        }
    }
}
