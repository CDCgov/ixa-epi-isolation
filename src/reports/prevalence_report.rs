use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, report::ContextReportExt,
    Context, ContextPeopleExt, ExecutionPhase, HashMap, HashMapExt, IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, str::FromStr};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    age: u8,
    symptoms: Option<SymptomValue>,
    infection_status: InfectionStatusValue,
    hospitalized: bool,
    count: usize,
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
    report_map_container: HashMap<PersonPropertyReportValues, usize>,
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

    let _ = *report_container_mut
        .report_map_container
        .entry(event.current)
        .and_modify(|n| *n += 1)
        .or_insert(1);

    let _ = *report_container_mut
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

impl FromStr for InfectionStatusValue {
    type Err = IxaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Susceptible" => Ok(InfectionStatusValue::Susceptible),
            "Infectious" => Ok(InfectionStatusValue::Infectious),
            "Recovered" => Ok(InfectionStatusValue::Recovered),
            _ => Err(IxaError::IxaError("Value type not found for infection status".to_string()))
        }
    }
}

impl FromStr for SymptomValue {
    type Err = IxaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Presymptomatic" => Ok(SymptomValue::Presymptomatic),
            "Category1" => Ok(SymptomValue::Category1),
            "Category2" => Ok(SymptomValue::Category2),
            "Category3" => Ok(SymptomValue::Category3),
            "Category4" => Ok(SymptomValue::Category4),
            _ => Err(IxaError::IxaError("Value type not found for symptom value".to_string())) 
        }
    }
}

pub fn init(
    context: &mut Context,
    file_name: &str,
    period: f64,
) -> Result<(), IxaError> {
    // Count initial number of people per property status
    context.add_report::<PersonPropertyReport>(file_name)?;

    let tabulator = (Age, InfectionStatus, Symptoms, Hospitalized);
    // Tabulate initial counts    
    let map_counts: RefCell<HashMap<PersonPropertyReportValues, usize>> = RefCell::new(HashMap::new());
    context.tabulate_person_properties(&tabulator, |_context, values, count| {
        // Handle the string Option<SymptomValue> 
        let symptoms = if let Some(start) = values[2].find('(') {
            let end = values[2].find(')').unwrap();
            let symptom_string = &values[2][(start + 1)..end];
            let symptom_values = symptom_string.parse::<SymptomValue>().unwrap();
            Some(symptom_values)
        } else {
            None
        };
        // Create the struct report values
        let input = PersonPropertyReportValues {
            age: values[0].parse::<u8>().unwrap(),
            infection_status: values[1].parse::<InfectionStatusValue>().unwrap(),
            symptoms,
            hospitalized: values[3].parse::<bool>().unwrap()
        };

        map_counts.borrow_mut().insert(input, count);
    });

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.report_map_container = map_counts.take();

    context.subscribe_to_event::<ReportEvent>(
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
