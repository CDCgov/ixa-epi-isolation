use crate::{infectiousness_manager::InfectedBy, parameters::ContextParametersExt};
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
    infected_by: PersonId,
    // setting_id: SettingId,
}

create_report_trait!(TransmissionReport);

trait TransmissionReportContextExt {
    fn record_transmission_event(&mut self, event: PersonPropertyChangeEvent<InfectedBy>);
    fn create_transmission_report(&mut self, file_name: &str) -> Result<(), IxaError>;
}

impl TransmissionReportContextExt for Context {
    fn record_transmission_event(&mut self, event: PersonPropertyChangeEvent<InfectedBy>) {
        let contact = event.person_id;
        let infector = self.get_person_property(contact, InfectedBy).unwrap();
        // let setting = get_person_setting_id(infector).unwrap();

        self.send_report(TransmissionReport {
            time: self.get_current_time(),
            target_id: contact,
            infected_by: infector,
            // setting_id: setting,
        });
    }

    fn create_transmission_report(&mut self, file_name: &str) -> Result<(), IxaError> {
        self.add_report::<TransmissionReport>(file_name)?;
        self.subscribe_to_event::<PersonPropertyChangeEvent<InfectedBy>>(|context, event| {
            context.record_transmission_event(event);
        });
        Ok(())
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params().clone();
    match parameters.transmission_report_name {
        Some(name) => {
            context.create_transmission_report(name.as_ref())?;
        }
        None => {
            info!("No transmission report name provided, skipping transmission report creation");
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
    use statrs::prec;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use super::TransmissionReport;

    fn load_json_params(path: PathBuf) -> Context {
        let mut context = Context::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        context.load_global_properties(&path).unwrap();
        context
    }

    #[test]
    fn test_empty_transmission_report() {
        let context = load_json_params(PathBuf::from(
            "tests/data/empty_transmission_report_test.json",
        ));
        let report_name = context.get_params().transmission_report_name.clone();
        assert!(report_name.is_none());
    }

    #[test]
    fn test_filled_transmission_report() {
        let context = load_json_params(PathBuf::from(
            "tests/data/filled_transmission_report_test.json",
        ));
        let report_name = context.get_params().transmission_report_name.clone();
        assert_eq!(report_name.unwrap(), "output.csv");
    }

    #[test]
    fn test_generate_transmission_report() {
        let mut context = load_json_params(PathBuf::from(
            "tests/data/filled_transmission_report_test.json",
        ));
        context.init_random(context.get_params().seed);
        context.add_rate_fn(Box::new(ConstantRate::new(1.0, 5.0)));

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        crate::transmission_report::init(&mut context).unwrap();

        let source = context.add_person(()).unwrap();
        let target = context.add_person(()).unwrap();
        let infection_time = 1.0;

        context.add_plan(infection_time, move |context| {
            context.create_transmission_event(source, target);
        });
        context.execute();

        let file_path = path.join("output.csv");

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        for result in reader.deserialize() {
            let record: TransmissionReport = result.unwrap();
            assert!(prec::almost_eq(record.time, infection_time, 1e-16));
            assert_eq!(record.target_id, target);
            assert_eq!(record.infected_by, source);
        }
    }
}
