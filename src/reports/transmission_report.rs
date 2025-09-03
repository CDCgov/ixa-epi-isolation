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
