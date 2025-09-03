use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    parameters::ContextParametersExt,
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, info, report::ContextReportExt,
    Context, ContextPeopleExt, ExecutionPhase, HashMap, IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    age: u8,
    symptoms: Option<SymptomValue>,
    infection_status: InfectionStatusValue,
    hospitalized: bool,
    count: u32,
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
    t_upper: f64,
    age: u8,
    event: String,
    count: u32,
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
    report_map_container: HashMap<PersonPropertyReportValues, u32>,
    incidence_infection_status: HashMap<(u8, InfectionStatusValue), u32>,
    incidence_symptoms: HashMap<(u8, SymptomValue), u32>,
    incidence_hospitalization: HashMap<(u8, bool), u32>,
    // Unused?
    // event_names: HashMap<String, HashMap<String, String>>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        report_map_container: HashMap::default(),
        incidence_infection_status: HashMap::default(),
        incidence_symptoms: HashMap::default(),
        incidence_hospitalization: HashMap::default(),
        // event_names: HashMap::default(),
    }
);

fn update_infection_incidence(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectionStatus>,
) {
    if event.current == InfectionStatusValue::Infectious
        || event.current == InfectionStatusValue::Recovered
    {
        let age = context.get_person_property(event.person_id, Age);
        let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
        report_container_mut
            .incidence_infection_status
            .entry((age, event.current))
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }
}

fn update_symptoms_incidence(context: &mut Context, event: PersonPropertyChangeEvent<Symptoms>) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
    if let Some(symptoms) = event.current {
        if symptoms != SymptomValue::Presymptomatic {
            report_container_mut
                .incidence_symptoms
                .entry((age, symptoms))
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    }
}

fn update_hospitalization_incidence(
    context: &mut Context,
    event: PersonPropertyChangeEvent<Hospitalized>,
) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
    if event.current {
        report_container_mut
            .incidence_hospitalization
            .entry((age, event.current))
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
}

fn update_property_change_counts(
    context: &mut Context,
    event: PersonPropertyChangeEvent<PersonReportProperties>,
) {
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);

    *report_container_mut
        .report_map_container
        .entry(event.current)
        .or_insert(0) += 1;

    *report_container_mut
        .report_map_container
        .entry(event.previous)
        .or_insert(0) -= 1;
}

fn reset_incidence_map(context: &mut Context) {
    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container
        .incidence_infection_status
        .values_mut()
        .for_each(|v| *v = 0);
    report_container
        .incidence_symptoms
        .values_mut()
        .for_each(|v| *v = 0);
    report_container
        .incidence_hospitalization
        .values_mut()
        .for_each(|v| *v = 0);
}

fn send_property_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);

    for (values, count_property) in &report_container.report_map_container {
        context.send_report(PersonPropertyReport {
            t: context.get_current_time(),
            age: values.age,
            infection_status: values.infection_status,
            symptoms: values.symptoms,
            hospitalized: values.hospitalized,
            count: *count_property,
        });
    }
}

fn send_incidence_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);
    let t_upper = context.get_current_time();

    // Infection status
    for ((age, infection_status), count) in &report_container.incidence_infection_status {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{infection_status:?}"),
            count: *count,
        });
    }
    // Symptoms
    for ((age, symptoms), count) in &report_container.incidence_symptoms {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{symptoms:?}"),
            count: *count,
        });
    }
    // Hospitalization
    for ((age, _), count) in &report_container.incidence_hospitalization {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("hospitalized"),
            count: *count,
        });
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
    let mut map_counts = HashMap::default();

    // This is a bit of a cheat, as we are not able to iterate over the entire population.
    for person in context.query_people((Hospitalized, false)) {
        let value = context.get_person_property(person, PersonReportProperties);
        map_counts
            .entry(value)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
    // Only if you have initially hospitalized people.
    for person in context.query_people((Hospitalized, true)) {
        let value = context.get_person_property(person, PersonReportProperties);
        map_counts
            .entry(value)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.report_map_container = map_counts;

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

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    for age in 0..100 {
        let inf_vec = [
            InfectionStatusValue::Infectious,
            InfectionStatusValue::Recovered,
        ];

        for inf_value in inf_vec {
            report_container
                .incidence_infection_status
                .insert((age, inf_value), 0);
        }

        let symp_vec = [
            SymptomValue::Category1,
            SymptomValue::Category2,
            SymptomValue::Category3,
            SymptomValue::Category4,
        ];
        for symp_value in symp_vec {
            report_container
                .incidence_symptoms
                .insert((age, symp_value), 0);
        }

        let hosp_vec = [true];
        for hosp_value in hosp_vec {
            report_container
                .incidence_hospitalization
                .insert((age, hosp_value), 0);
        }
    }

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
