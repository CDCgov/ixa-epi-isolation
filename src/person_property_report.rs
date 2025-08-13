use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    parameters::ContextParametersExt,
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, info, report::ContextReportExt,
    Context, ContextPeopleExt, ExecutionPhase, IxaError, PersonPropertyChangeEvent, HashMap
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
    report_container.incidence_infection_status.clear();
    report_container.incidence_symptoms.clear();
    report_container.incidence_hospitalization.clear();
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
            event: format!("{:?}", infection_status),
            count: *count,
        });
    }
    // Symptoms
    for ((age, symptoms), count) in &report_container.incidence_symptoms {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{:?}", symptoms),
            count: *count,
        });
    }
    // Hospitalization
    for ((age, hospitalization), count) in &report_container.incidence_hospitalization {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{:?}", hospitalization),
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
        map_counts.entry(value).and_modify(|count| *count += 1).or_insert(1);
    }
    // Only if you have initially hospitalized people.
    for person in context.query_people((Hospitalized, true)) {
        let value = context.get_person_property(person, PersonReportProperties);
        map_counts.entry(value).and_modify(|count| *count += 1).or_insert(1);
    }

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container
        .report_map_container = map_counts;

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

    // Is the point of this to have zero counts for all values?
    /*let mut incidence_counts: HashMap<u8, HashMap<String, HashMap<String, usize>>> = HashMap::default();
    let mut names_map: HashMap<String, HashMap<String, String>> = HashMap::default();

    for age in 0..100 {
        let inf_vec = [
            (InfectionStatusValue::Infectious, "Infectious".to_string()),
            (InfectionStatusValue::Recovered, "Recovered".to_string()),
        ];

        for inf_value in inf_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("InfectionStatus".to_string())
                .or_default()
                .insert(format!("{:?}", inf_value.0), 0);
            names_map
                .entry("InfectionStatus".to_string())
                .or_default()
                .insert(format!("{:?}", inf_value.0), inf_value.1);
        }

        let symp_vec = [
            (
                Some(SymptomValue::Category1),
                "Symptom-Category1".to_string(),
            ),
            (
                Some(SymptomValue::Category2),
                "Symptom-Category2".to_string(),
            ),
            (
                Some(SymptomValue::Category3),
                "Symptom-Category3".to_string(),
            ),
            (
                Some(SymptomValue::Category4),
                "Symptom-Category4".to_string(),
            ),
        ];
        for symp_value in symp_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("Symptoms".to_string())
                .or_default()
                .insert(format!("{:?}", symp_value.0), 0);
            names_map
                .entry("Symptoms".to_string())
                .or_default()
                .insert(format!("{:?}", symp_value.0), symp_value.1);
        }

        let hosp_vec = [(true, "Hospitalized".to_string())];
        for hosp_value in hosp_vec {
            incidence_counts
                .entry(age)
                .or_default()
                .entry("Hospitalized".to_string())
                .or_default()
                .insert(format!("{:?}", hosp_value.0), 0);
            names_map
                .entry("Hospitalized".to_string())
                .or_default()
                .insert(format!("{:?}", hosp_value.0), hosp_value.1);
        }
    }

    let report_container = context.get_data_mut(PropertyReportDataPlugin);

    report_container
        .report_incidence_container
        .clone_from(&incidence_counts);
    report_container.event_names.clone_from(&names_map);
    */

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
