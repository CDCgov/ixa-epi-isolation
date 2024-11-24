use crate::population_loader::{Age, CensusTract};

use crate::Parameters;
use ixa::{
    context::Context,
    create_report_trait, define_data_plugin,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonCreatedEvent},
    report::{ContextReportExt, Report},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct PersonReportItem {
    time: f64,
    age: u8,
    population: usize,
    census_tract: usize,
}

#[derive(Clone)]
struct PopulationReportData {
    census_tract_set: HashSet<usize>,
}

define_data_plugin!(
    PopulationReportPlugin,
    PopulationReportData,
    PopulationReportData {
        census_tract_set: HashSet::new(),
    }
);

create_report_trait!(PersonReportItem);

fn send_population_report(context: &mut Context, report_period: f64) {
    let population_data = context.get_data_container_mut(PopulationReportPlugin);

    let current_census_set = population_data.census_tract_set.clone();
    for age_it in 0..100 {
        for tract in &current_census_set {
            let age_pop = context
                .query_people(((Age, age_it), (CensusTract, (*tract))))
                .len();
            context.send_report(PersonReportItem {
                time: context.get_current_time(),
                age: age_it,
                population: age_pop,
                census_tract: *tract,
            });
        }
    }

    context.add_plan(context.get_current_time() + report_period, move |context| {
        send_population_report(context, report_period);
    });
}

fn update_property_set(context: &mut Context, event: PersonCreatedEvent) {
    let person_census = context.get_person_property(event.person_id, CensusTract);
    let report_plugin = context.get_data_container_mut(PopulationReportPlugin);
    report_plugin.census_tract_set.insert(person_census);
}

pub fn init(context: &mut Context, output_dir: &Path) -> Result<(), IxaError> {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    context
        .report_options()
        .overwrite(true)
        .directory(PathBuf::from(output_dir));

    context.subscribe_to_event(|context, event: PersonCreatedEvent| {
        update_property_set(context, event);
    });

    context.add_report::<PersonReportItem>(&parameters.output_file)?;
    context.add_plan(0.0, move |context| {
        send_population_report(context, parameters.report_period);
    });
    Ok(())
}
