use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    parameters::ContextParametersExt,
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, info, report::ContextReportExt,
    Context, ContextPeopleExt, ExecutionPhase, IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    age: String,
    symptoms: String,
    infection_status: String,
    hospitalized: String,
    count: usize,
}

#[derive(Eq, Hash, PartialEq, Serialize, Deserialize, Copy, Clone, Debug)]
pub struct PersonPropertyReportValues {
    age: u8,
    symptoms: Option<SymptomValue>,
    infection_status: InfectionStatusValue,
    hospitalized: bool,
}

define_report!(PersonPropertyReport);

define_derived_property!(
    PersonReportProperties,
    PersonPropertyReportValues,
    [Age, Symptoms, InfectionStatus, Hospitalized],
    |age, symptoms, infection_status, hospitalized| {
        PersonPropertyReportValues {
            age,
            symptoms,
            infection_status,
            hospitalized,
        }
    }
);

struct PropertyReportDataContainer {
    person_property_report_map_container: HashMap<Vec<String>, usize>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        person_property_report_map_container: HashMap::new(),
    }
);

fn update_property_change_counts(
    context: &mut Context,
    event: PersonPropertyChangeEvent<PersonReportProperties>,
) {
    let previous_vec = vec![
        format!("{:?}", event.previous.age),
        format!("{:?}", event.previous.symptoms),
        format!("{:?}", event.previous.infection_status),
        format!("{:?}", event.previous.hospitalized),
    ];
    let current_vec = vec![
        format!("{:?}", event.current.age),
        format!("{:?}", event.current.symptoms),
        format!("{:?}", event.current.infection_status),
        format!("{:?}", event.current.hospitalized),
    ];
    let report_container_mut = context.get_data_container_mut(PropertyReportDataPlugin);

    *report_container_mut
        .person_property_report_map_container
        .entry(current_vec)
        .or_insert(0) += 1;

    *report_container_mut
        .person_property_report_map_container
        .entry(previous_vec)
        .or_insert(0) -= 1;
}

fn send_property_counts(context: &mut Context) {
    let report_container = context
        .get_data_container(PropertyReportDataPlugin)
        .unwrap();

    for (values, count_property) in &report_container.person_property_report_map_container {
        context.send_report(PersonPropertyReport {
            t: context.get_current_time(),
            age: values[0].clone(),
            symptoms: values[1].clone(),
            infection_status: values[2].clone(),
            hospitalized: values[3].clone(),
            count: *count_property,
        });
    }
}

fn create_person_property_report(
    context: &mut Context,
    file_name: &str,
    period: f64,
) -> Result<(), IxaError> {
    // Count initial number of people per property status
    context.add_report::<PersonPropertyReport>(file_name)?;

    // Compute initial counts
    let map_counts: RefCell<HashMap<Vec<String>, usize>> = RefCell::new(HashMap::new());
    context.tabulate_person_properties(
        &(Age, Symptoms, InfectionStatus, Hospitalized),
        |_context, values, count| {
            map_counts.borrow_mut().insert(values.to_vec(), count);
        },
    );
    let report_container = context.get_data_container_mut(PropertyReportDataPlugin);
    report_container
        .person_property_report_map_container
        .clone_from(&map_counts.borrow());

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_property_counts(context);
        },
        ExecutionPhase::Last,
    );

    context.subscribe_to_event::<PersonPropertyChangeEvent<PersonReportProperties>>(
        |context, event| {
            update_property_change_counts(context, event);
        },
    );
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    let person_property_report_name = parameters.person_property_report_name.clone();
    let report_period = parameters.report_period;
    match person_property_report_name {
        Some(path_name) => {
            create_person_property_report(context, path_name.as_ref(), report_period)?;
        }
        None => {
            info!("No property report name provided. Skipping report creation");
        }
    }
    Ok(())
}
