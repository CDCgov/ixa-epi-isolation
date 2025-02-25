use crate::{
    infectiousness_manager::{InfectionData, InfectionDataValue},
    parameters::ContextParametersExt,
};
use ixa::{
    create_report_trait, info,
    report::{ContextReportExt, Report},
    Context, ContextPeopleExt, IxaError, PersonId, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct TransmissionReport {
    time: f64,
    target_id: PersonId,
    infected_by: Option<PersonId>,
    // setting_id: SettingId,
}

create_report_trait!(TransmissionReport);

trait TransmissionReportContextExt {
    fn record_transmission_event(&mut self, event: PersonPropertyChangeEvent<InfectionData>);
    fn create_transmission_report(&mut self, file_name: &str) -> Result<(), IxaError>;
}

impl TransmissionReportContextExt for Context {
    fn record_transmission_event(&mut self, event: PersonPropertyChangeEvent<InfectionData>) {
        let target_id = event.person_id;
        let InfectionDataValue::Infected { infected_by, .. } =
            self.get_person_property(target_id, InfectionData)
        else {
            panic!("Person {target_id} is not infected")
        };
        // let setting = get_person_setting_id(infector).unwrap();

        self.send_report(TransmissionReport {
            time: self.get_current_time(),
            target_id,
            infected_by,
            // setting_id: setting,
        });
    }

    fn create_transmission_report(&mut self, file_name: &str) -> Result<(), IxaError> {
        self.add_report::<TransmissionReport>(file_name)?;
        self.subscribe_to_event::<PersonPropertyChangeEvent<InfectionData>>(|context, event| {
            context.record_transmission_event(event);
        });
        Ok(())
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    let report = parameters.transmission_report_name.clone();
    match report {
        Some(path_name) => {
            context.create_transmission_report(path_name.as_ref())?;
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
        infectiousness_manager::InfectionContextExt,
        parameters::ContextParametersExt,
        rate_fns::{rate_fn_storage::InfectiousnessRateExt, ConstantRate},
    };
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    };
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    use super::TransmissionReport;

    fn setup_context_from_path(path: &Path) -> Context {
        let mut context = Context::new();
        context.load_global_properties(path).unwrap();
        context.init_random(context.get_params().seed);
        context.add_rate_fn(Box::new(ConstantRate::new(1.0, 5.0)));
        context
    }

    #[test]
    fn test_empty_transmission_report() {
        let path = PathBuf::from("tests/data/empty_transmission_report_test.json");
        let context = setup_context_from_path(path.as_ref());
        let report_name = context.get_params().transmission_report_name.clone();
        assert!(report_name.is_none());
    }

    #[test]
    fn test_filled_transmission_report() {
        let path = PathBuf::from("tests/data/filled_transmission_report_test.json");
        let context = setup_context_from_path(path.as_ref());
        let report_name = context.get_params().transmission_report_name.clone();
        assert_eq!(report_name.unwrap(), "output.csv");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_generate_transmission_report() {
        let path = PathBuf::from("tests/data/filled_transmission_report_test.json");
        let mut context = setup_context_from_path(path.as_ref());

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        crate::transmission_report::init(&mut context).unwrap();

        let source = context.add_person(()).unwrap();
        let target = context.add_person(()).unwrap();
        let infection_time = 1.0;

        context.infect_person(source, None);

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source));
        });
        context.execute();

        let file_path = path.join("output.csv");

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        for result in reader.deserialize() {
            let record: TransmissionReport = result.unwrap();
            assert_eq!(record.time, infection_time);
            assert_eq!(record.target_id, target);
            assert_eq!(record.infected_by.unwrap(), source);
        }
    }
}
