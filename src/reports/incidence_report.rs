use crate::{
    hospitalizations::Hospitalized,
    infectiousness_manager::{InfectionStatus, InfectionStatusValue},
    population_loader::Age,
    symptom_progression::{SymptomValue, Symptoms},
};
use ixa::{
    define_data_plugin, define_report, report::ContextReportExt, Context, ContextPeopleExt,
    ExecutionPhase, HashMap, HashSet, HashSetExt, IxaError, PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

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
    hospitalization: HashMap<u8, u32>,
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
            .entry(age)
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
    for (age, count) in &report_container.hospitalization {
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
///
/// # Panics
///
/// Will panic if an age group cannot be parsed from the tabulated string
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), IxaError> {
    context.add_report::<PersonPropertyIncidenceReport>(file_name)?;

    let tabulator = (Age,);
    let ages: RefCell<HashSet<u8>> = RefCell::new(HashSet::new());
    context.tabulate_person_properties(&tabulator, |_context, values, _count| {
        ages.borrow_mut().insert(values[0].parse::<u8>().unwrap());
    });

    let report_container = context.get_data_mut(PropertyReportDataPlugin);

    for age in ages.take() {
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

        report_container.hospitalization.insert(age, 0);
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

#[cfg(test)]
mod test {
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, Params},
        rate_fns::load_rate_fns,
        reports::ReportParams,
        Age,
    };
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(incidence_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    incidence_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_generate_incidence_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            name: Some("output.csv".to_string()),
            period: Some(2.0),
        });

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

        let Params {
            incidence_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = incidence_report.name {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::incidence_report::PersonPropertyIncidenceReport =
                result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 && record.event == *"Infectious" && record.age == 43 {
                assert_eq!(record.count, 1);
            } else {
                assert_eq!(record.count, 0);
            }
        }

        // 7 event types: 4 symptom categories + hospitalization + Infectious + Recovered
        // 2 time points
        // 2 ages
        assert_eq!(line_count, 28);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_age_change() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            name: Some("output.csv".to_string()),
            period: Some(2.0),
        });

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
        context.add_plan(infection_time - 0.1, move |context| {
            context.set_person_property(target, Age, 44);
        });
        context.execute();

        let Params {
            incidence_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = incidence_report.name {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::incidence_report::PersonPropertyIncidenceReport =
                result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 && record.event == *"Infectious" && record.age == 44 {
                assert_eq!(record.count, 1);
            } else {
                assert_eq!(record.count, 0);
            }
        }

        // 7 event types: 4 symptom categories + hospitalization + Infectious + Recovered
        // 2 time points
        // 2 ages at first timepoint, 3 ages at second timepoint for only one event (7x2x2 + 1 = 29)
        assert_eq!(line_count, 29);
    }
}
