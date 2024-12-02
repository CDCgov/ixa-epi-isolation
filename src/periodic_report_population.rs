use crate::population_loader::{Age, CensusTract};

use crate::Parameters;
use ixa::{
    context::{Context, ExecutionPhase},
    create_report_trait, define_data_plugin,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonCreatedEvent},
    report::{ContextReportExt, Report},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

use std::collections::HashSet;

#[derive(Serialize)]
struct PopulationReportItem {
    time: f64,
    census_tract: usize,
    age: u8,
    population: usize,
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

create_report_trait!(PopulationReportItem);

fn send_population_report(context: &mut Context) {
    let population_data = context.get_data_container(PopulationReportPlugin).unwrap();

    let current_census_set = &population_data.census_tract_set;
    for age_it in 0..100 {
        for tract in current_census_set {
            let age_pop = context
                .query_people(((Age, age_it), (CensusTract, (*tract))))
                .len();
            context.send_report(PopulationReportItem {
                time: context.get_current_time(),
                census_tract: *tract,
                age: age_it,
                population: age_pop,
            });
        }
    }
}

fn update_property_set(context: &mut Context, event: PersonCreatedEvent) {
    let person_census = context.get_person_property(event.person_id, CensusTract);
    let report_plugin = context.get_data_container_mut(PopulationReportPlugin);
    report_plugin.census_tract_set.insert(person_census);
}

pub fn init(
    context: &mut Context,
    output_dir: &Path,
    force_overwrite: bool,
) -> Result<(), IxaError> {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    context
        .report_options()
        .overwrite(force_overwrite)
        .directory(PathBuf::from(output_dir));

    context.subscribe_to_event(|context, event: PersonCreatedEvent| {
        update_property_set(context, event);
    });

    context.add_report::<PopulationReportItem>(&parameters.population_periodic_report)?;
    context.add_periodic_plan_with_phase(
        parameters.report_period,
        move |context| {
            send_population_report(context);
        },
        ExecutionPhase::Last,
    );
    Ok(())
}
