use crate::{
    infectiousness_manager::{InfectionData, InfectionDataValue},
    parameters::ContextParametersExt,
};
use ixa::{
    define_report, info, report::ContextReportExt, Context, IxaError, PersonId,
    PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};
use std::string::ToString;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct TransmissionReport {
    time: f64,
    target_id: PersonId,
    infected_by: Option<PersonId>,
    infection_setting_type: Option<String>,
    infection_setting_id: Option<usize>,
}

define_report!(TransmissionReport);

fn record_transmission_event(
    context: &mut Context,
    target_id: PersonId,
    infected_by: Option<PersonId>,
    infection_setting_type: Option<String>,
    infection_setting_id: Option<usize>,
) {
    if infected_by.is_some() {
        context.send_report(TransmissionReport {
            time: context.get_current_time(),
            target_id,
            infected_by,
            infection_setting_type,
            infection_setting_id,
        });
    }
}

fn create_transmission_report(context: &mut Context, file_name: &str) -> Result<(), IxaError> {
    context.add_report::<TransmissionReport>(file_name)?;
    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionData>>(|context, event| {
        if let InfectionDataValue::Infectious {
            infected_by,
            infection_setting_type,
            infection_setting_id,
            ..
        } = event.current
        {
            record_transmission_event(
                context,
                event.person_id,
                infected_by,
                infection_setting_type.map(ToString::to_string),
                infection_setting_id,
            );
        }
    });
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    let report = parameters.transmission_report_name.clone();
    match report {
        Some(path_name) => {
            create_transmission_report(context, path_name.as_ref())?;
        }
        None => {
            info!("No transmission report name provided. Skipping transmission report creation");
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use crate::{
        infectiousness_manager::InfectionContextExt, parameters::ContextParametersExt,
        rate_fns::load_rate_fns,
    };
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt, assert_almost_eq
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    use super::TransmissionReport;
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
    fn test_empty_transmission_report() {
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
    fn test_filled_transmission_report() {
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
                "transmission_report_name": "output.csv"
                }
            }
        "#;
        let context = setup_context_from_str(params_json);
        let report_name = context.get_params().transmission_report_name.clone();
        assert_eq!(report_name.unwrap(), "output.csv".to_string());
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
                "transmission_report_name": "output.csv"
                }
            }
        "#;
        let mut context = setup_context_from_str(params_json);

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        crate::transmission_report::init(&mut context).unwrap();

        let source = context.add_person(()).unwrap();
        let target = context.add_person(()).unwrap();
        let setting_type = Some("test_setting");
        let setting_id: Option<usize> = Some(1);
        let infection_time = 1.0;

        context.infect_person(source, None, None, None);

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting_type, setting_id);
        });
        context.execute();

        let file_path = path.join("output.csv");

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        for result in reader.deserialize() {
            let record: TransmissionReport = result.unwrap();
            assert_almost_eq!(record.time, infection_time, 0.0);
            assert_eq!(record.target_id, target);
            assert_eq!(record.infected_by.unwrap(), source);
            assert_eq!(
                record.infection_setting_type,
                Some("test_setting".to_string())
            );
            assert_eq!(record.infection_setting_id, Some(1));
        }
    }
}
