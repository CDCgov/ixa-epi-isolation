use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_report, report::ContextReportExt, Context, ContextPeopleExt,
    ExecutionPhase, HashMap, IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyIncidenceReport {
    t_upper: f64,
    age: u8,
    event: String,
    count: u32,
}

define_report!(PersonPropertyIncidenceReport);

struct PropertyReportDataContainer {
    infection_status_change: HashMap<(u8, InfectionStatusValue), u32>,
    symptom_onset: HashMap<(u8, SymptomValue), u32>,
    hospitalization: HashMap<(u8, bool), u32>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        infection_status_change: HashMap::default(),
        symptom_onset: HashMap::default(),
        hospitalization: HashMap::default(),
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
            .infection_status_change
            .entry((age, event.current))
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }
}

fn update_symptoms_incidence(context: &mut Context, event: PersonPropertyChangeEvent<Symptoms>) {
    let age = context.get_person_property(event.person_id, Age);
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);

    // Only track true symptom onset, not Some(Presymptomatic) or None (symptom resolution)
    match event.current {
        None | Some(SymptomValue::Presymptomatic) => (),
        Some(symptoms) => {
            report_container_mut
                .symptom_onset
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
    // Only track hospitalization, not departure
    if event.current {
        report_container_mut
            .hospitalization
            .entry((age, event.current))
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
}

fn reset_incidence_map(context: &mut Context) {
    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container
        .infection_status_change
        .values_mut()
        .for_each(|v| *v = 0);
    report_container
        .symptom_onset
        .values_mut()
        .for_each(|v| *v = 0);
    report_container
        .hospitalization
        .values_mut()
        .for_each(|v| *v = 0);
}

fn send_incidence_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);
    let t_upper = context.get_current_time();

    // Infection status
    for ((age, infection_status), count) in &report_container.infection_status_change {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{infection_status:?}"),
            count: *count,
        });
    }
    // Symptoms
    for ((age, symptoms), count) in &report_container.symptom_onset {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{symptoms:?}"),
            count: *count,
        });
    }
    // Hospitalization
    for ((age, _), count) in &report_container.hospitalization {
        // We only ever record entering the hospital, we print a string to avoid an ambiguous boolean value
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: "Hospitalized".to_string(),
            count: *count,
        });
    }
    reset_incidence_map(context);
}

/// # Errors
///
/// Will return `IxaError` if the report cannot be added
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), IxaError> {
    context.add_report::<PersonPropertyIncidenceReport>(file_name)?;

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    for age in 0..100 {
        let inf_vec = [
            InfectionStatusValue::Infectious,
            InfectionStatusValue::Recovered,
        ];

        for inf_value in inf_vec {
            report_container
                .infection_status_change
                .insert((age, inf_value), 0);
        }

        let symp_vec = [
            SymptomValue::Category1,
            SymptomValue::Category2,
            SymptomValue::Category3,
            SymptomValue::Category4,
        ];
        for symp_value in symp_vec {
            report_container.symptom_onset.insert((age, symp_value), 0);
        }

        report_container.hospitalization.insert((age, true), 0);
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
