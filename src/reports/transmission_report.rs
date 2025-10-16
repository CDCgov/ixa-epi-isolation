use crate::infectiousness_manager::{InfectionData, InfectionDataValue};
use crate::profiling::open_span;
use ixa::{
    define_report, report::ContextReportExt, Context, IxaError, PersonId, PersonPropertyChangeEvent,
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

/// # Errors
///
/// Will return `IxaError` if the report cannot be added
pub fn init(context: &mut Context, file_name: &str) -> Result<(), IxaError> {
    context.add_report::<TransmissionReport>(file_name)?;
    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionData>>(|context, event| {
        let _span = open_span("transmission_report");
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

#[cfg(test)]
mod test {
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, Params},
        rate_fns::load_rate_fns,
        reports::ReportParams,
    };
    use ixa::assert_almost_eq;
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(transmission_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 10.0,
                    transmission_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_transmission_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: None,
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let source = context.add_person(()).unwrap();
        let target = context.add_person(()).unwrap();
        let setting_type = Some("test_setting");
        let setting_id: Option<usize> = Some(1);
        let infection_time = 1.0;

        context.infect_person(source, None, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting_type, setting_id);
        });
        context.execute();

        let Params {
            transmission_report,
            ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = transmission_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::transmission_report::TransmissionReport = result.unwrap();
            assert_almost_eq!(record.time, infection_time, 0.0);
            assert_eq!(record.target_id, target);
            assert_eq!(record.infected_by.unwrap(), source);
            assert_eq!(
                record.infection_setting_type,
                Some("test_setting".to_string())
            );
            assert_eq!(record.infection_setting_id, setting_id);
            line_count += 1;
        }
        assert_eq!(line_count, 1);
    }
}
