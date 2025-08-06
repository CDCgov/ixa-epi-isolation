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
    infection_status: InfectionStatusValue,
    symptoms: Option<SymptomValue>,
    hospitalized: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyIncidenceReport {
    t: f64,
    age: u8,
    property: String,
    property_value: String,
    count: usize,
}

define_report!(PersonPropertyReport);
define_report!(PersonPropertyIncidenceReport);

define_derived_property!(
    PersonReportProperties,
    PersonPropertyReportValues,
    [Age, InfectionStatus, Symptoms, Hospitalized],
    |age, infection_status, symptoms, hospitalized| {
        PersonPropertyReportValues {
            age,
            infection_status,
            symptoms,
            hospitalized,
        }
    }
);

struct PropertyReportDataContainer {
    person_property_report_map_container: HashMap<Vec<String>, usize>,
    person_property_report_incidence_container:
        HashMap<u8, HashMap<String, HashMap<String, usize>>>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        person_property_report_map_container: HashMap::new(),
        person_property_report_incidence_container: HashMap::new(),
    }
);

fn update_infection_incidence(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectionStatus>,
) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
    report_container_mut
        .person_property_report_incidence_container
        .entry(age)
        .and_modify(|inf_map| {
            inf_map
                .entry("InfectionStatus".to_string())
                .and_modify(|count_map| {
                    *count_map.entry(format!("{:?}", event.current)).or_insert(0) += 1;
                });
        });
}

fn update_symptoms_incidence(context: &mut Context, event: PersonPropertyChangeEvent<Symptoms>) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
    report_container_mut
        .person_property_report_incidence_container
        .entry(age)
        .and_modify(|symp_map| {
            symp_map
                .entry("Symptoms".to_string())
                .and_modify(|count_map| {
                    *count_map.entry(format!("{:?}", event.current)).or_insert(0) += 1;
                });
        });
}

fn update_hospitalization_incidence(
    context: &mut Context,
    event: PersonPropertyChangeEvent<Hospitalized>,
) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
    report_container_mut
        .person_property_report_incidence_container
        .entry(age)
        .and_modify(|hosp_map| {
            hosp_map
                .entry("Hospitalized".to_string())
                .and_modify(|count_map| {
                    *count_map.entry(format!("{:?}", event.current)).or_insert(0) += 1;
                });
        });
}

fn update_property_change_counts(
    context: &mut Context,
    event: PersonPropertyChangeEvent<PersonReportProperties>,
) {
    let previous_vec = vec![
        format!("{:?}", event.previous.age),
        format!("{:?}", event.previous.infection_status),
        format!("{:?}", event.previous.symptoms),
        format!("{:?}", event.previous.hospitalized),
    ];
    let current_vec = vec![
        format!("{:?}", event.current.age),
        format!("{:?}", event.current.infection_status),
        format!("{:?}", event.current.symptoms),
        format!("{:?}", event.current.hospitalized),
    ];
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);

    *report_container_mut
        .person_property_report_map_container
        .entry(current_vec.clone())
        .or_insert(0) += 1;

    *report_container_mut
        .person_property_report_map_container
        .entry(previous_vec)
        .or_insert(0) -= 1;
}

fn reset_incidence_map(context: &mut Context) {
    let report_container = context.get_data_mut(PropertyReportDataPlugin);

    #[allow(clippy::explicit_iter_loop)]
    for (_, map_incidence) in report_container
        .person_property_report_incidence_container
        .iter_mut()
    {
        for (_, property_map) in map_incidence.iter_mut() {
            for (_, count_property) in property_map.iter_mut() {
                *count_property = 0;
            }
        }
    }
}

fn send_property_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);

    for (values, count_property) in &report_container.person_property_report_map_container {
        context.send_report(PersonPropertyReport {
            t: context.get_current_time(),
            age: values[0].clone(),
            infection_status: values[1].clone(),
            symptoms: values[2].clone(),
            hospitalized: values[3].clone(),
            count: *count_property,
        });
    }
}

fn send_incidence_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);

    for (age, map_incidence) in &report_container.person_property_report_incidence_container {
        for (property_name, property_map) in map_incidence {
            for (value, count_property) in property_map {
                context.send_report(PersonPropertyIncidenceReport {
                    t: context.get_current_time(),
                    age: *age,
                    property: property_name.clone(),
                    property_value: value.clone(),
                    count: *count_property,
                });
            }
        }
    }
    reset_incidence_map(context);
}

fn create_prevalence_report(
    context: &mut Context,
    file_name: &str,
    period: f64,
) -> Result<(), IxaError> {
    // Count initial number of people per property status
    context.add_report::<PersonPropertyReport>(file_name)?;

    // Compute initial counts
    let map_counts: RefCell<HashMap<Vec<String>, usize>> = RefCell::new(HashMap::new());
    context.tabulate_person_properties(
        &(Age, InfectionStatus, Symptoms, Hospitalized),
        |_context, values, count| {
            map_counts.borrow_mut().insert(values.to_vec(), count);
        },
    );
    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container
        .person_property_report_map_container
        .clone_from(&map_counts.borrow());

    context.subscribe_to_event::<PersonPropertyChangeEvent<PersonReportProperties>>(
        |context, event| {
            update_property_change_counts(context, event);
        },
    );

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_property_counts(context);
        },
        ExecutionPhase::Last,
    );
    Ok(())
}

fn create_incidence_report(
    context: &mut Context,
    file_name: &str,
    period: f64,
) -> Result<(), IxaError> {
    context.add_report::<PersonPropertyIncidenceReport>(file_name)?;

    let mut incidence_counts: HashMap<u8, HashMap<String, HashMap<String, usize>>> = HashMap::new();

    for age in 0..100 {
        let inf_vec = [
            InfectionStatusValue::Susceptible,
            InfectionStatusValue::Infectious,
            InfectionStatusValue::Recovered,
        ];

        for inf_value in inf_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("InfectionStatus".to_string())
                .or_default()
                .insert(format!("{inf_value:?}"), 0);
        }

        let symp_vec = [
            None,
            Some(SymptomValue::Presymptomatic),
            Some(SymptomValue::Category1),
            Some(SymptomValue::Category2),
            Some(SymptomValue::Category3),
            Some(SymptomValue::Category4),
        ];
        for symp_value in symp_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("Symptoms".to_string())
                .or_default()
                .insert(format!("{symp_value:?}"), 0);
        }

        let hosp_vec = [false, true];
        for hosp_value in hosp_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("Hospitalized".to_string())
                .or_default()
                .insert(format!("{hosp_value:?}"), 0);
        }
    }

    let report_container = context.get_data_mut(PropertyReportDataPlugin);

    report_container
        .person_property_report_incidence_container
        .clone_from(&incidence_counts);

    context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(|context, event| {
        update_infection_incidence(context, event);
    });
    context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(|context, event| {
        update_symptoms_incidence(context, event);
    });
    context.subscribe_to_event::<PersonPropertyChangeEvent<Hospitalized>>(|context, event| {
        update_hospitalization_incidence(context, event);
    });

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_incidence_counts(context);
        },
        ExecutionPhase::Last,
    );

    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    let person_property_report_name = parameters.person_property_report_name.clone();
    let report_period = parameters.report_period;
    match person_property_report_name {
        Some(path_name) => {
            create_prevalence_report(context, path_name.as_ref(), report_period)?;
            create_incidence_report(
                context,
                ("incidence_".to_owned() + &path_name).as_ref(),
                report_period,
            )?;
        }
        None => {
            info!("No property report name provided. Skipping report creation");
        }
    }
    Ok(())
}
