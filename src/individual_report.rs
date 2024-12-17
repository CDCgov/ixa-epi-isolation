use ixa::context::Context;
use ixa::error::IxaError;
use ixa::people::{ContextPeopleExt, PersonPropertyChangeEvent};
use ixa::report::ContextReportExt;
use ixa::{create_report_trait, report::Report};
use std::path::PathBuf;

use crate::population_loader::Age;
use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct ReportItem {
    time: f64,
    person_id: String,
    age: u8,
    updated_status: InfectiousStatusType,
    previous_status: InfectiousStatusType,
}

create_report_trait!(ReportItem);

fn handle_infection_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    let age_person = context.get_person_property(event.person_id, Age);
    context.send_report(ReportItem {
        time: context.get_current_time(),
        person_id: format!("{}", event.person_id),
        age: age_person,
        updated_status: event.current,
        previous_status: event.previous,
    });
}

pub fn init(context: &mut Context, output_path: PathBuf) -> Result<(), IxaError> {
    context.report_options().directory(output_path);

    context.add_report::<ReportItem>("individual_report")?;
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
            handle_infection_status_change(context, event);
        },
    );
    Ok(())
}
