use crate::{
    hospitalizations::{ContextHospitalizationExt, Hospitalized},
    parameters::ContextParametersExt,
};
use ixa::{
    create_report_trait, info,
    report::{ContextReportExt, Report},
    Context, IxaError, PersonId, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct HospitalIncidenceReport {
    time: f64,
    person_id: PersonId,
    hospital_id: usize,
}

create_report_trait!(HospitalIncidenceReport);

fn record_hospital_incidence_event(context: &mut Context, person_id: PersonId, hospital_id: usize) {
    context.send_report(HospitalIncidenceReport {
        time: context.get_current_time(),
        person_id,
        hospital_id,
    });
}

fn create_hospital_incidence_report(
    context: &mut Context,
    file_name: &str,
) -> Result<(), IxaError> {
    context.add_report::<HospitalIncidenceReport>(file_name)?;
    context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(|context, event| {
        if event.current {
            let hospital_id = context.get_hospital_id();
            record_hospital_incidence_event(context, event.person_id, hospital_id);
        }
    });
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let report_name = context
        .get_params()
        .hospitalization_parameters
        .as_ref()
        .and_then(|hospital_parameters| hospital_parameters.hospital_incidence_report_name.clone());

    if let Some(report) = report_name {
        create_hospital_incidence_report(context, &report)?;
    } else {
        info!("No hospital incidence report name provided. Skipping hospital incidence report creation");
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use crate::hospitalizations::Hospitalized;
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
                    "probability_by_age": [1.0, 1.0, 1.0],
                    "mean_delay_to_hospitalization": 1.0,
                    "mean_duration_of_hospitalization": 1.0,
                    "hospital_incidence_report_name": "hospital_incidence_report.csv"
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
        assert_eq!(
            report_name.unwrap(),
            "hospital_incidence_report.csv".to_string()
        );
    }

    #[test]
    fn test_generate_transmission_report() {
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
                    "probability_by_age": [1.0, 1.0, 1.0],
                    "mean_delay_to_hospitalization": 1.0,
                    "mean_duration_of_hospitalization": 1.0,
                    "hospital_incidence_report_name": "hospital_incidence_report.csv"
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

        let person_id = context.add_person(()).unwrap();
        let hospital_id: usize = 0;
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
            assert_eq!(record.hospital_id, hospital_id);
        }
    }
}
