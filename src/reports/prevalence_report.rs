use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, report::ContextReportExt,
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

define_report!(PersonPropertyReport);

#[derive(Eq, Hash, PartialEq, Serialize, Deserialize, Copy, Clone, Debug)]
pub struct PersonPropertyReportValues {
    age: u8,
    infection_status: InfectionStatusValue,
    symptoms: Option<SymptomValue>,
    hospitalized: bool,
}

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
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        report_map_container: HashMap::default(),
    }
);

type ReportEvent = PersonPropertyChangeEvent<PersonReportProperties>;

fn update_property_change_counts(
    context: &mut Context,
    event: ReportEvent,
) {
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);

    *report_container_mut
        .report_map_container
        .entry(event.current)
        .and_modify(|n| *n += 1)
        .or_insert(1);

    *report_container_mut
        .report_map_container
        .entry(event.previous)
        .and_modify(|n| *n -= 1)
        .or_insert(0);
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

pub fn init(
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
