use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, report::ContextReportExt, Context,
    ContextPeopleExt, ExecutionPhase, HashMap, HashMapExt, IxaError, PersonPropertyChangeEvent,
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

fn update_property_change_counts(context: &mut Context, event: ReportEvent) {
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
            _ => Err(IxaError::IxaError(
                "Value type not found for infection status".to_string(),
            )),
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
            _ => Err(IxaError::IxaError(
                "Value type not found for symptom value".to_string(),
            )),
        }
    }
}

/// # Errors
///
/// Will return `IxaError` if the report cannot be added
///
/// # Panics
///
/// Will panic if symptom value string is not listed in enum
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), IxaError> {
    // Count initial number of people per property status
    context.add_report::<PersonPropertyReport>(file_name)?;

    let tabulator = (Age, InfectionStatus, Symptoms, Hospitalized);
    // Tabulate initial counts
    let map_counts: RefCell<HashMap<PersonPropertyReportValues, usize>> =
        RefCell::new(HashMap::new());
    context.tabulate_person_properties(&tabulator, |_context, values, count| {
        // Handle the string Option<SymptomValue>
        let symptoms = if values[2].starts_with("Some") {
            let end = values[2].chars().count();
            // Get symptom value contents within Some(_)
            let symptom_string = &values[2][6..(end - 1)];
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
            hospitalized: values[3].parse::<bool>().unwrap(),
        };

        map_counts.borrow_mut().insert(input, count);
    });

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.report_map_container = map_counts.take();

    context.subscribe_to_event::<ReportEvent>(|context, event| {
        update_property_change_counts(context, event);
    });

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_property_counts(context);
        },
        ExecutionPhase::Last,
    );
    Ok(())
}


#[cfg(test)]
mod test {
    use crate::{
        Age,
        infectiousness_manager::InfectionContextExt, parameters::{ContextParametersExt, GlobalParams, Params},
        rate_fns::load_rate_fns, reports::ReportType
    };
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;


    fn setup_context_with_report(report: ReportType) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    reports: vec![report],
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_prevalence_report() {
        let mut context = setup_context_with_report(ReportType::PrevalenceReport {name: "output.csv".to_string(), period: 2.0});

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let source = context.add_person((Age, 42)).unwrap();
        let target = context.add_person((Age, 43)).unwrap();
        let setting_type = Some("test_setting");
        let setting_id: Option<usize> = Some(1);
        let infection_time = 1.0;

        context.infect_person(source, None, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting_type, setting_id);
        });
        context.execute();

        
        let Params { reports, .. } = context.get_params();
        let file_path = path.join(match &reports[0] {
            ReportType::PrevalenceReport { name , ..} => name,
            _ => panic!("Unreachable report encountered")
        });

        assert!(file_path.exists());
        std::mem::drop(context);

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();

        let mut actual: Vec<Vec<String>> = reader
            .records()
            .map(|result| result.unwrap().iter().map(String::from).collect())
            .collect();
        let mut expected = vec![
            //   t    | age |sym| inf status  | hosp   | count
            vec!["0.0", "42", "", "Infectious", "false", "1"],
            vec!["0.0", "43", "", "Infectious", "false", "0"],
            vec!["0.0", "42", "", "Susceptible", "false", "0"],
            vec!["0.0", "43", "", "Susceptible", "false", "1"],
            vec!["2.0", "42", "", "Infectious", "false", "1"],
            vec!["2.0", "43", "", "Infectious", "false", "1"],
            vec!["2.0", "42", "", "Susceptible", "false", "0"],
            vec!["2.0", "43", "", "Susceptible", "false", "0"],
        ];

        actual.sort();
        expected.sort();

        assert_eq!(actual, expected, "CSV file should contain the correct data");
    }
}
